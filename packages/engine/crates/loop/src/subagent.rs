//! Subagent spawning: the engine-owned execution behind the `Task` tool.
//!
//! The Task tool ships only a descriptor; the loop intercepts calls to it and
//! runs them here. A subagent is a plain child session of the same
//! [`NativeAgent`] (its own log, own turn), given its role's model chain,
//! filtered tools, and a self-contained brief. Its final message is returned
//! to the parent as the tool's output; its events relay live into the parent
//! stream so a client can render the tree.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, ContentBlock, Effort, ModelRef, PermissionMode, PromptInput, SessionId, ToolCallId,
    ToolCallStatus, ToolOutput, TurnOptions, TurnStopReason, now_ms,
};
use agentloop_core::tool::{SUBMIT_VERDICT_TOOL_NAME, ToolError};

use crate::agent::NativeAgent;
use crate::roles::VERIFIER_ROLE;

/// Cap on a child result folded back into the parent's context.
const RESULT_MAX_CHARS: usize = 24_000;

/// One subagent to spawn, assembled by the Task tool intercept.
pub(crate) struct SubagentRequest {
    /// The parent Task tool call id (for `SubagentStarted.call_id`).
    pub call_id: ToolCallId,
    /// Role name; must be spawnable (not `main`).
    pub role: String,
    /// Short UI label.
    pub description: String,
    /// The self-contained task brief (first user message).
    pub prompt: String,
    /// What the child should return, if the model specified it.
    pub expected_output: Option<String>,
    /// Model chosen by split round-robin; `None` = the role chain's first
    /// resolvable model, else the parent's inherited model.
    pub assigned_model: Option<ModelRef>,
    /// Permission mode inherited from the parent turn.
    pub permission_mode: Option<PermissionMode>,
    /// Effort inherited from the parent turn; the child's own role then scales
    /// the derived thinking budget in `run_iteration`.
    pub effort: Option<Effort>,
    /// The parent call's cancel token — cancellation cascades to the child.
    pub cancel: CancellationToken,
}

impl NativeAgent {
    /// Run one subagent to completion, returning its final message as the
    /// Task tool's output. Relays the child's events into `parent`.
    pub(crate) async fn run_subagent(
        self: &Arc<Self>,
        parent: &SessionId,
        req: SubagentRequest,
    ) -> Result<ToolOutput, ToolError> {
        let Some(role) = self.deps.roles.get(&req.role).cloned() else {
            let available = self
                .deps
                .roles
                .spawnable()
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(ToolError::InvalidInput(format!(
                "unknown role `{}`. Available roles: {available}.",
                req.role
            )));
        };

        let mut chain: Vec<ModelRef> = Vec::new();
        if let Some(model) = req.assigned_model.clone() {
            chain.push(model);
        }
        for model in &role.models {
            if !chain.contains(model) {
                chain.push(model.clone());
            }
        }

        let parent_meta = self
            .deps
            .store
            .get_meta(parent)
            .await
            .map_err(|err| ToolError::Execution(err.to_string()))?;
        let child_model = chain.first().cloned().or_else(|| parent_meta.model.clone());

        let child = SessionId::generate();

        // Per-subagent isolation: a role that asks for isolation gets its own
        // worktree branched from the parent's cwd, merged back (or discarded)
        // when the child finishes. Failures degrade per policy, mirroring
        // root-session provisioning in `create_session`.
        let policy = role.isolation;
        let mut child_cwd = parent_meta.cwd.clone();
        let mut isolation = None;
        let mut workspace_id = None;
        let mut base_cwd = None;
        let mut provisioned_event = None;
        if policy.wants_isolation() {
            match &self.deps.workspace {
                Some(backend) => match backend.provision(&parent_meta.cwd, &child, policy).await {
                    Ok(Some(workspace)) => {
                        child_cwd = workspace.root.clone();
                        isolation = Some(policy);
                        workspace_id = Some(workspace.id.clone());
                        base_cwd = Some(parent_meta.cwd.clone());
                        provisioned_event = Some(AgentEvent::WorkspaceProvisioned {
                            workspace_id: workspace.id,
                            path: workspace.root,
                            base_ref: workspace.base_ref,
                        });
                    }
                    Ok(None) => tracing::warn!(
                        target: "workspace", session = %child,
                        "subagent isolation optional but base cannot be isolated; sharing parent cwd"
                    ),
                    Err(err) if policy.is_required() => {
                        return Err(ToolError::Execution(format!(
                            "role `{}` requires an isolated workspace but provisioning failed: {err}",
                            req.role
                        )));
                    }
                    Err(err) => tracing::warn!(
                        target: "workspace", session = %child,
                        "subagent isolation failed; sharing parent cwd: {err}"
                    ),
                },
                None if policy.is_required() => {
                    return Err(ToolError::Execution(format!(
                        "role `{}` requires an isolated workspace but no workspace backend \
                         is configured",
                        req.role
                    )));
                }
                None => tracing::warn!(
                    target: "workspace", session = %child,
                    "subagent isolation requested but no workspace backend configured; \
                     sharing parent cwd"
                ),
            }
        }

        let now = now_ms();
        let meta = agentloop_contracts::SessionMeta {
            id: child.clone(),
            title: Some(req.description.clone()),
            agent_id: self.deps.agent_id.clone(),
            parent_id: Some(parent.clone()),
            role: Some(req.role.clone()),
            depth: parent_meta.depth.saturating_add(1),
            provider_session_id: None,
            cwd: child_cwd,
            model: child_model,
            fallback_models: chain.iter().skip(1).cloned().collect(),
            mode: None,
            isolation,
            workspace_id,
            executor: parent_meta.executor.clone(),
            base_cwd,
            created_at_ms: now,
            updated_at_ms: now,
        };
        self.deps
            .store
            .create(meta.clone())
            .await
            .map_err(|err| ToolError::Execution(err.to_string()))?;
        let child_workspace = meta
            .workspace_id
            .clone()
            .map(|id| (id, meta.cwd.clone(), parent_meta.cwd.clone()));
        let child_handle = self.install_child_handle(&child);
        child_handle
            .emit_persistent(None, AgentEvent::SessionCreated { meta })
            .await
            .map_err(|err| ToolError::Execution(err.to_string()))?;
        if let Some(event) = provisioned_event {
            let _ = child_handle.emit_persistent(None, event).await;
        }

        let parent_handle = self
            .live_handle(parent)
            .ok_or_else(|| ToolError::Execution("parent session is not live".to_owned()))?;
        let _ = parent_handle
            .emit_persistent(
                None,
                AgentEvent::SubagentStarted {
                    child_session: child.clone(),
                    task: req.description.clone(),
                    call_id: Some(req.call_id.clone()),
                    role: Some(req.role.clone()),
                },
            )
            .await;

        let relay_stop = CancellationToken::new();
        let relay = self.spawn_relay(&child_handle, parent_handle.clone(), &child, &relay_stop);

        let brief = build_brief(&req, &role.prompt);
        let opts = TurnOptions {
            model: chain.first().cloned(),
            fallback_models: chain.iter().skip(1).cloned().collect(),
            permission_mode: req.permission_mode,
            system_append: role.prompt.clone(),
            effort: req.effort,
            ..TurnOptions::default()
        };
        let summary = tokio::select! {
            biased;
            _ = req.cancel.cancelled() => {
                child_handle.request_cancel();
                Err(ToolError::Cancelled)
            }
            result = Box::pin(crate::turn::run_turn(&self.deps, child_handle.clone(), PromptInput::text(brief), opts)) => {
                result.map_err(|err| ToolError::Execution(err.to_string()))
            }
        };

        relay_stop.cancel();
        let _ = relay.await;

        // Tear the child's workspace down: merge completed work back into the
        // parent's tree, discard on error/cancellation. Integration problems
        // never fail the tool call — they're folded into the result text so
        // the parent can react.
        let mut integration_note = None;
        if let Some((workspace_id, root, base)) = child_workspace {
            if let Some(backend) = &self.deps.workspace {
                let completed = matches!(
                    &summary,
                    Ok(s) if s.stop_reason == TurnStopReason::EndTurn
                );
                if completed {
                    match backend.integrate(&root, &base, None).await {
                        Ok(outcome) => {
                            let _ = parent_handle
                                .emit_persistent(
                                    None,
                                    AgentEvent::WorkspaceIntegrated {
                                        workspace_id,
                                        outcome: outcome.clone(),
                                    },
                                )
                                .await;
                            integration_note = match outcome {
                                agentloop_contracts::IntegrationOutcome::Merged {
                                    files_changed,
                                } => Some(format!(
                                    "[workspace: merged {files_changed} changed file(s) back \
                                     into {}]",
                                    base.display()
                                )),
                                agentloop_contracts::IntegrationOutcome::Empty => None,
                                agentloop_contracts::IntegrationOutcome::VerifyFailed {
                                    detail,
                                } => Some(format!(
                                    "[workspace: NOT merged, verify failed: {detail}; worktree \
                                     kept at {}]",
                                    root.display()
                                )),
                                agentloop_contracts::IntegrationOutcome::Diverged { branch } => {
                                    Some(format!(
                                        "[workspace: NOT merged, base diverged; work kept on \
                                         branch `{branch}`]"
                                    ))
                                }
                                _ => None,
                            };
                        }
                        Err(err) => {
                            integration_note = Some(format!(
                                "[workspace: integration failed: {err}; worktree kept at {}]",
                                root.display()
                            ));
                        }
                    }
                } else if let Err(err) = backend.discard(&root, &base).await {
                    tracing::warn!(
                        target: "workspace", session = %child,
                        "failed to discard subagent workspace: {err}"
                    );
                }
            }
        }

        let summary = summary?;
        let _ = parent_handle
            .emit_persistent(
                None,
                AgentEvent::SubagentCompleted {
                    child_session: child.clone(),
                    summary: summary.clone(),
                },
            )
            .await;

        if summary.stop_reason == TurnStopReason::Cancelled {
            return Err(ToolError::Cancelled);
        }

        let final_text = self.collect_final_text(&child).await;
        let is_error = matches!(
            summary.stop_reason,
            TurnStopReason::Error | TurnStopReason::MaxIterations
        );
        let mut text = if final_text.trim().is_empty() {
            format!(
                "(subagent finished with no textual output: {:?})",
                summary.stop_reason
            )
        } else {
            final_text
        };
        if let Some(note) = integration_note {
            text.push_str("\n\n");
            text.push_str(&note);
        }
        let mut output = if is_error {
            ToolOutput::error(text)
        } else {
            ToolOutput::text(text)
        };
        if req.role == VERIFIER_ROLE {
            output.structured = self.extract_last_verdict(&child).await;
        }
        Ok(output)
    }

    /// Read the child log for its last completed `SubmitVerdict` call and
    /// return the structured verdict it reported, so a `Verify` call's
    /// `ToolOutput` carries a machine-readable outcome — not just prose —
    /// even though the verifier itself only ever produces free text plus
    /// one tool call.
    async fn extract_last_verdict(&self, child: &SessionId) -> Option<serde_json::Value> {
        let events = self.deps.store.read(child, 0).await.ok()?;
        events.iter().rev().find_map(|(_, event)| match event {
            AgentEvent::ToolCallUpdated { call }
                if call.tool_name == SUBMIT_VERDICT_TOOL_NAME
                    && call.status == ToolCallStatus::Completed =>
            {
                call.result
                    .as_ref()
                    .and_then(|output| output.structured.clone())
            }
            _ => None,
        })
    }

    /// Read the child log and join its last assistant message, capped.
    async fn collect_final_text(&self, child: &SessionId) -> String {
        let Ok(events) = self.deps.store.read(child, 0).await else {
            return String::new();
        };
        let mut text = String::new();
        for (_, event) in &events {
            if let AgentEvent::AssistantMessage { content, .. } = event {
                let joined = content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Markdown { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if !joined.trim().is_empty() {
                    text = joined;
                }
            }
        }
        if text.len() > RESULT_MAX_CHARS {
            let cut = text
                .char_indices()
                .take_while(|(idx, _)| *idx < RESULT_MAX_CHARS)
                .last()
                .map(|(idx, ch)| idx + ch.len_utf8())
                .unwrap_or(RESULT_MAX_CHARS);
            let dropped = text.len() - cut;
            text.truncate(cut);
            text.push_str(&format!(
                "\n[… truncated {dropped} chars — spawn a narrower follow-up task if more detail is needed]"
            ));
        }
        text
    }
}

/// Assemble the child's first user message from the brief and expected output.
fn build_brief(req: &SubagentRequest, _role_prompt: &Option<String>) -> String {
    let expected = req
        .expected_output
        .clone()
        .unwrap_or_else(|| "A concise, complete report; see the return contract below.".to_owned());
    format!(
        "# Task ({role}): {description}\n\n{prompt}\n\n\
         ## Expected output\n{expected}\n\n\
         ## Return contract\n\
         You are a subagent. Your FINAL message is the only thing returned to the \
         agent that spawned you — there is no follow-up conversation. Include \
         everything the expected output asks for, cite absolute file paths (and \
         line numbers for code claims), state any assumptions you made, and keep \
         it token-efficient. If you could not finish, say exactly what remains.",
        role = req.role,
        description = req.description,
        prompt = req.prompt,
        expected = expected,
    )
}
