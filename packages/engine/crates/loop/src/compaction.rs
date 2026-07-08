//! Context compaction: summarize conversation history so future turns send a
//! compressed prefix instead of the full transcript.

use std::sync::Arc;

use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use agentloop_contracts::{
    AgentEvent, CompactionSummary, Message, Role, Transcript, TranscriptItem, TurnOptions,
    markdown, reduce,
};
use agentloop_core::provider::ChatRequest;
use agentloop_core::{AgentError, ProviderError};

use crate::context_budget::resolve_context_limit;
use crate::deps::TurnDeps;
use crate::draft::AssistantDraft;
use crate::session_handle::SessionHandle;

const SUMMARIZE_SYSTEM: &str = "You are a conversation summarizer. Summarize the following \
    conversation history into a concise but complete markdown summary. Preserve: key decisions, \
    file paths, code changes, errors, and open tasks. Omit filler and redundant tool output. \
    Output only the summary — no preamble or closing remarks.";

/// Summarize the session's current context view and record a compaction boundary.
pub(crate) async fn compact_session(
    deps: &Arc<TurnDeps>,
    handle: Arc<SessionHandle>,
    opts: TurnOptions,
    cancel: CancellationToken,
    strategy: &str,
) -> Result<CompactionSummary, AgentError> {
    let events = deps.store.read(&handle.id, 0).await?;
    let transcript = reduce(events.iter().map(|(_, event)| event).collect::<Vec<_>>());
    let items_before = transcript.items.len();

    let meta = deps.store.get_meta(&handle.id).await?;
    let model_ref = opts
        .model
        .clone()
        .or_else(|| meta.model.clone())
        .or_else(|| deps.default_model.clone())
        .ok_or_else(|| {
            AgentError::Other(
                "no model configured: pass TurnOptions.model, set a session model, \
                 or configure a default model"
                    .to_owned(),
            )
        })?;
    let (provider, model) = deps.providers.resolve(&model_ref).ok_or_else(|| {
        AgentError::Other(format!(
            "no provider registered for model reference `{model_ref}`; \
             registered providers: {:?}",
            deps.providers.ids()
        ))
    })?;

    let context_limit = resolve_context_limit(&provider);
    let source = compact_source_text(&transcript, context_limit);
    if source.trim().is_empty() {
        return Err(AgentError::Other(
            "nothing to compact — start a conversation first".to_owned(),
        ));
    }

    let mut request = ChatRequest::new(
        model.clone(),
        vec![Message {
            role: Role::User,
            content: vec![agentloop_contracts::ContentBlock::markdown(format!(
                "Summarize this conversation history:\n\n{source}"
            ))],
            cache_hint: false,
        }],
    );
    request.system = Some(SUMMARIZE_SYSTEM.to_owned());

    let tokens_before = estimate_tokens(&source);

    let mut stream = provider.stream_chat(request, cancel.clone()).await?;
    let mut draft = AssistantDraft::new();
    while let Some(event) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled {
                provider: provider.id(),
            }
            .into());
        }
        let event = event?;
        draft.apply(event);
    }

    let (content, _) = draft.finish();
    let summary_markdown = content
        .iter()
        .filter_map(|block| match block {
            agentloop_contracts::ContentBlock::Markdown { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_owned();

    if summary_markdown.is_empty() {
        return Err(AgentError::Other(
            "compaction produced an empty summary".to_owned(),
        ));
    }

    let tokens_after = estimate_tokens(&summary_markdown);
    let summary = CompactionSummary {
        summary_markdown,
        strategy: strategy.to_owned(),
        tokens_before: Some(tokens_before),
        tokens_after: Some(tokens_after),
    };

    handle
        .emit_persistent(
            None,
            AgentEvent::CompactionBoundary {
                summary: summary.clone(),
            },
        )
        .await?;

    tracing::info!(
        target: "loop",
        session_id = %handle.id,
        items_before,
        tokens_before,
        tokens_after,
        "context compacted"
    );

    Ok(summary)
}

/// Build the text fed to the summarizer: the prior compaction summary (kept
/// whole) plus as many of the *newest* tail items as fit within a char budget
/// derived from the model's context window. Older overflowing items are
/// dropped so the summarization request itself never exceeds the limit.
///
/// The budget targets ~half the window (`estimate_tokens` is ~4 chars/token,
/// so `limit/2` tokens ≈ `limit * 2` chars), leaving room for the system
/// prompt, the request wrapper, and the model's summary output.
fn compact_source_text(transcript: &Transcript, context_limit: u64) -> String {
    let (prior, tail) = transcript.context_view();

    let prefix = prior.map(|summary| {
        format!(
            "## Prior summary (from earlier compaction)\n\n{}",
            summary.summary_markdown
        )
    });

    let max_source_chars = context_limit.saturating_mul(2) as usize;
    let budget = max_source_chars.saturating_sub(prefix.as_ref().map_or(0, |p| p.len() + 2));

    let mut kept: Vec<TranscriptItem> = Vec::new();
    let mut used = 0usize;
    let mut dropped = false;
    for item in tail.iter().rev() {
        let len = render_item(item).len() + 2;
        if !kept.is_empty() && used + len > budget {
            dropped = true;
            break;
        }
        used += len;
        kept.push(item.clone());
    }
    kept.reverse();

    let mut parts = Vec::new();
    if let Some(prefix) = prefix {
        parts.push(prefix);
    }
    if dropped {
        parts.push("## (older messages omitted to fit the summarization window)".to_owned());
    }
    if !kept.is_empty() {
        parts.push(markdown::transcript_to_markdown(&Transcript {
            items: kept,
            compaction: None,
            boundary_index: None,
        }));
    }
    parts.join("\n\n")
}

/// Render a single transcript item to markdown (for per-item budget accounting).
fn render_item(item: &TranscriptItem) -> String {
    markdown::transcript_to_markdown(&Transcript {
        items: vec![item.clone()],
        compaction: None,
        boundary_index: None,
    })
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{CompactionSummary, TranscriptBlock, TranscriptItem};

    #[test]
    fn compact_source_includes_prior_summary_and_tail() {
        let transcript = Transcript {
            items: vec![TranscriptItem {
                message_id: agentloop_contracts::MessageId::from("m1"),
                role: Role::User,
                blocks: vec![TranscriptBlock::Markdown {
                    text: "new question".to_owned(),
                }],
                model: None,
                usage: None,
            }],
            compaction: Some(CompactionSummary {
                summary_markdown: "old stuff".to_owned(),
                strategy: "summarize_oldest".to_owned(),
                tokens_before: None,
                tokens_after: None,
            }),
            boundary_index: Some(0),
        };
        let source = compact_source_text(&transcript, 1_000_000);
        assert!(source.contains("old stuff"));
        assert!(source.contains("new question"));
    }

    #[test]
    fn compact_source_drops_oldest_tail_when_over_budget() {
        let items: Vec<TranscriptItem> = (0..50)
            .map(|i| TranscriptItem {
                message_id: agentloop_contracts::MessageId::from(format!("m{i}")),
                role: Role::User,
                blocks: vec![TranscriptBlock::Markdown {
                    text: format!("message number {i} with some filler content to add length"),
                }],
                model: None,
                usage: None,
            })
            .collect();
        let transcript = Transcript {
            items,
            compaction: None,
            boundary_index: None,
        };
        let source = compact_source_text(&transcript, 100);
        assert!(source.contains("older messages omitted"));
        assert!(source.contains("message number 49"));
        assert!(!source.contains("message number 0 "));
    }
}
