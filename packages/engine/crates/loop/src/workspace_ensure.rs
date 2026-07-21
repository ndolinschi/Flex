//! First-prompt workspace provisioning for isolated root sessions.
//!
//! When a session is created with an isolation policy that wants isolation,
//! `NativeAgent::create_session` deliberately does not spawn a worktree — it
//! records the policy (and any `reuse_workspace_id` hint) on `SessionMeta`
//! but leaves `cwd` pointed at the project directory. The actual provision
//! (or attach) happens here, on the first turn, so:
//!
//! - The UI has time between create and first prompt to let the user pick
//!   an existing workspace to reuse (or fall through to a fresh one).
//! - `create_session` stays off the `git` hot path.
//! - A session that never sees a prompt never leaves a worktree behind.
//!
//! Once provisioned we patch the session meta with `cwd` (the worktree
//! root), `workspace_id`, `base_cwd` (the original project directory), and
//! clear `reuse_workspace_id`; then emit a persistent
//! [`AgentEvent::WorkspaceProvisioned`] so the transcript records the same
//! event shape as an eager-provisioned session.

use std::sync::Arc;

use agentloop_contracts::{AgentEvent, SessionMeta, SessionMetaPatch};
use agentloop_core::AgentError;

use crate::deps::TurnDeps;
use crate::session_handle::SessionHandle;

/// If `meta` is a depth-0 session whose isolation policy wants a workspace
/// but hasn't been provisioned yet, provision (or attach a reuse hint) and
/// return the updated `SessionMeta` — otherwise return `meta` unchanged.
/// Emits a persistent `WorkspaceProvisioned` on success.
///
/// Failure semantics mirror `create_session`: `Required` propagates the
/// error; `Optional` logs and continues in place.
pub(crate) async fn ensure_root_workspace(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    meta: SessionMeta,
) -> Result<SessionMeta, AgentError> {
    if meta.depth != 0 {
        return Ok(meta);
    }
    if meta.workspace_id.is_some() {
        return Ok(meta);
    }
    let Some(policy) = meta.isolation.filter(|p| p.wants_isolation()) else {
        return Ok(meta);
    };
    let base_cwd = meta.cwd.clone();
    let reuse = meta.reuse_workspace_id.clone();

    let Some(backend) = deps.workspace.as_ref() else {
        if policy.is_required() {
            return Err(AgentError::Other(
                "isolation required but no workspace backend is configured".to_owned(),
            ));
        }
        tracing::warn!(
            target: "workspace", session = %meta.id,
            "isolation requested but no workspace backend configured; continuing in place"
        );
        return Ok(meta);
    };

    // Reuse an existing worktree if a hint was provided, else provision fresh.
    let outcome = match reuse.as_deref() {
        Some(id) => backend.attach(&base_cwd, id, &meta.id, policy).await,
        None => backend.provision(&base_cwd, &meta.id, policy).await,
    };

    let workspace = match outcome {
        Ok(Some(workspace)) => workspace,
        Ok(None) => {
            tracing::warn!(
                target: "workspace", session = %meta.id,
                "isolation optional but base cannot be isolated; continuing in place"
            );
            return Ok(meta);
        }
        Err(err) if policy.is_required() => {
            return Err(AgentError::Other(format!(
                "isolation required but could not be provisioned: {err}"
            )));
        }
        Err(err) => {
            tracing::warn!(
                target: "workspace", session = %meta.id,
                "isolation failed; continuing in place: {err}"
            );
            return Ok(meta);
        }
    };

    let workspace_root = workspace.root.clone();
    let workspace_id = workspace.id.clone();
    let base_ref = workspace.base_ref.clone();

    // Persist the new coordinates. Empty string / empty path clears the
    // pending reuse hint — see `SessionMetaPatch` docs.
    deps.store
        .update_meta(
            &meta.id,
            SessionMetaPatch {
                cwd: Some(workspace_root.clone()),
                workspace_id: Some(workspace_id.clone()),
                base_cwd: Some(base_cwd.clone()),
                reuse_workspace_id: Some(String::new()),
                ..Default::default()
            },
        )
        .await?;

    handle
        .emit_persistent(
            None,
            AgentEvent::WorkspaceProvisioned {
                workspace_id: workspace_id.clone(),
                path: workspace_root.clone(),
                base_ref,
            },
        )
        .await?;

    tracing::info!(
        target: "workspace",
        session = %meta.id,
        worktree = %workspace_root.display(),
        workspace_id = %workspace_id,
        attached = reuse.is_some(),
        "isolated workspace ready on first turn"
    );

    // Return the updated meta so the caller doesn't need to re-read the
    // store; callers that already read `meta.cwd` before this call must
    // switch to the returned value.
    Ok(SessionMeta {
        cwd: workspace_root,
        workspace_id: Some(workspace_id),
        base_cwd: Some(base_cwd),
        reuse_workspace_id: None,
        updated_at_ms: meta.updated_at_ms,
        ..meta
    })
}
