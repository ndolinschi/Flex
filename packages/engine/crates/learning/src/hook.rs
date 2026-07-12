//! The reflection hook: once per session, when the model stops with no tool
//! calls, inject a continuation asking it to distill at most one learned
//! skill — or explicitly do nothing when the session taught nothing new.

use std::collections::HashSet;
use std::sync::Mutex;

use async_trait::async_trait;

use agentloop_contracts::{HookPoint, SessionId};
use agentloop_core::{Hook, HookContext, HookData, HookError, HookOutcome};

/// Memory-progression schema a worthwhile distillation should walk through:
/// what broke → why → what you verified fixes it → the rule that generalizes.
/// Skipping straight to "save something" tends to record the incident, not
/// the lesson — the rule is what makes it useful next time, not the story.
const REFLECTION_PROMPT: &str = "Before finishing, check whether this session is worth \
distilling: did something fail, or did you work out a non-obvious multi-step procedure \
likely to recur? If so, walk it through — (1) what failed or was unclear, (2) the root \
cause, (3) the fact you verified fixes or explains it, (4) the general rule that follows \
— then save that procedure with the `SkillSave` tool (at most one skill; the rule, not \
just the incident). If nothing meets that bar (the common case), don't call any tool and \
simply restate your final answer.";

/// Injects the reflection continuation exactly once per session.
pub struct SkillLearningHook {
    prompted: Mutex<HashSet<SessionId>>,
}

impl SkillLearningHook {
    pub fn new() -> Self {
        Self {
            prompted: Mutex::new(HashSet::new()),
        }
    }
}

impl Default for SkillLearningHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Hook for SkillLearningHook {
    fn interests(&self) -> &[HookPoint] {
        &[HookPoint::Stop]
    }

    async fn on(
        &self,
        _point: HookPoint,
        ctx: &mut HookContext<'_>,
    ) -> Result<HookOutcome, HookError> {
        let HookData::Stop { continuation } = &mut ctx.data else {
            return Ok(HookOutcome::Continue);
        };
        // Never fight another hook's continuation.
        if continuation.is_some() {
            return Ok(HookOutcome::Continue);
        }
        let first_time = match self.prompted.lock() {
            Ok(mut prompted) => prompted.insert(ctx.session_id.clone()),
            Err(_) => false,
        };
        if !first_time {
            return Ok(HookOutcome::Continue);
        }
        **continuation = Some(REFLECTION_PROMPT.to_owned());
        Ok(HookOutcome::Mutated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop_ctx<'a>(
        session: &'a SessionId,
        continuation: &'a mut Option<String>,
    ) -> HookContext<'a> {
        HookContext {
            session_id: session,
            turn_id: None,
            data: HookData::Stop { continuation },
            store: None,
        }
    }

    #[tokio::test]
    async fn prompts_once_per_session() {
        let hook = SkillLearningHook::new();
        let session = SessionId::from("sess-1");

        let mut continuation = None;
        let outcome = hook
            .on(HookPoint::Stop, &mut stop_ctx(&session, &mut continuation))
            .await
            .expect("hook ok");
        assert_eq!(outcome, HookOutcome::Mutated);
        assert!(continuation.is_some());

        let mut second = None;
        let outcome = hook
            .on(HookPoint::Stop, &mut stop_ctx(&session, &mut second))
            .await
            .expect("hook ok");
        assert_eq!(outcome, HookOutcome::Continue);
        assert!(second.is_none());

        let other = SessionId::from("sess-2");
        let mut third = None;
        hook.on(HookPoint::Stop, &mut stop_ctx(&other, &mut third))
            .await
            .expect("hook ok");
        assert!(third.is_some());
    }

    #[tokio::test]
    async fn defers_to_an_existing_continuation() {
        let hook = SkillLearningHook::new();
        let session = SessionId::from("sess-1");
        let mut continuation = Some("someone else's".to_owned());
        let outcome = hook
            .on(HookPoint::Stop, &mut stop_ctx(&session, &mut continuation))
            .await
            .expect("hook ok");
        assert_eq!(outcome, HookOutcome::Continue);
        assert_eq!(continuation.as_deref(), Some("someone else's"));
    }
}
