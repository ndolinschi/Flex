//! Hook dispatch for a native turn.

use std::sync::Arc;

use agentloop_contracts::{AgentEvent, HookOutcomeKind, HookPoint, TurnId};
use agentloop_core::AgentError;
use agentloop_core::hook::{HookContext, HookData, HookOutcome};

use crate::deps::TurnDeps;
use crate::session_handle::SessionHandle;

/// Run all hooks interested in `point`; first `Block` wins. Non-`Continue`
/// outcomes are recorded in the event stream.
pub(super) async fn run_hooks(
    deps: &Arc<TurnDeps>,
    handle: &Arc<SessionHandle>,
    point: HookPoint,
    turn_id: &TurnId,
    mut data: HookData<'_>,
) -> Result<HookOutcome, AgentError> {
    let mut aggregate = HookOutcome::Continue;
    for hook in &deps.hooks {
        if !hook.interests().contains(&point) {
            continue;
        }
        let mut ctx = HookContext {
            session_id: &handle.id,
            turn_id: Some(turn_id),
            data: std::mem::replace(&mut data, HookData::Session),
            store: Some(deps.store.clone()),
        };
        let outcome = hook
            .on(point, &mut ctx)
            .await
            .map_err(|err| AgentError::Other(err.to_string()))?;
        data = ctx.data;
        match &outcome {
            HookOutcome::Continue => {}
            HookOutcome::Block { .. } => {
                handle.emit_ephemeral(
                    Some(turn_id),
                    AgentEvent::HookFired {
                        point,
                        outcome: HookOutcomeKind::Block,
                    },
                );
                return Ok(outcome);
            }
            HookOutcome::Mutated => {
                handle.emit_ephemeral(
                    Some(turn_id),
                    AgentEvent::HookFired {
                        point,
                        outcome: HookOutcomeKind::Mutated,
                    },
                );
                aggregate = HookOutcome::Mutated;
            }
        }
    }
    Ok(aggregate)
}
