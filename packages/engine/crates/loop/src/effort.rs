use agentloop_contracts::Effort;

const ROLE_SEARCHER: &str = "searcher";
const ROLE_REVIEWER: &str = "reviewer";

fn base_budget(effort: Effort) -> Option<u32> {
    match effort {
        Effort::Low => None,
        Effort::Medium => Some(8_192),
        Effort::High => Some(16_384),
        Effort::XHigh => Some(32_768),
        Effort::Max => Some(65_536),
    }
}

pub fn thinking_budget(effort: Effort, role: Option<&str>) -> Option<u32> {
    match role {
        Some(ROLE_SEARCHER) => base_budget(effort).map(|b| (b / 2).max(4_096)),
        Some(ROLE_REVIEWER) => {
            let floor = base_budget(Effort::High).unwrap_or(16_384);
            Some(base_budget(effort).unwrap_or(0).max(floor))
        }
        _ => base_budget(effort),
    }
}

pub fn guidance(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => {
            "Effort: low — favor speed. Answer directly and briefly. Skip planning and extra \
             file reads for straightforward work; use the fewest tool calls that get it right. \
             Do not over-explain."
        }
        Effort::Medium => {
            "Effort: medium — balance speed and rigor. Sketch a brief plan when a task has \
             multiple steps, read what you need to be correct, and verify the change you made."
        }
        Effort::High => {
            "Effort: high — think before acting. Read the code paths your change touches, plan \
             multi-step work, and verify with the narrowest real check. Prefer being right over \
             being fast."
        }
        Effort::XHigh => {
            "Effort: xhigh — long-horizon or tricky work. Explore the relevant code broadly \
             before changing it, reason about edge cases and failure modes, and decompose \
             separable work across parallel Task subagents (emit them in one turn). After \
             substantial changes you MUST spawn a fresh reviewer subagent before reporting done."
        }
        Effort::Max => {
            "Effort: maximum — correctness dominates cost. Exhaustively explore before acting, \
             enumerate and check edge cases, fan out independent work across parallel subagents, \
             and cross-verify: run tests AND spawn a fresh reviewer pass. Do not stop at the \
             first plausible solution — confirm it with independent evidence."
        }
    }
}

pub fn reasoning_effort_str(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High | Effort::XHigh | Effort::Max => "high",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_and_worker_budgets_track_the_level() {
        for role in [None, Some("worker")] {
            assert_eq!(thinking_budget(Effort::Low, role), None);
            assert_eq!(thinking_budget(Effort::Medium, role), Some(8_192));
            assert_eq!(thinking_budget(Effort::High, role), Some(16_384));
            assert_eq!(thinking_budget(Effort::XHigh, role), Some(32_768));
            assert_eq!(thinking_budget(Effort::Max, role), Some(65_536));
        }
    }

    #[test]
    fn searcher_gets_half_budget_floored() {
        let s = Some(ROLE_SEARCHER);
        assert_eq!(thinking_budget(Effort::Low, s), None);
        assert_eq!(thinking_budget(Effort::Medium, s), Some(4_096));
        assert_eq!(thinking_budget(Effort::High, s), Some(8_192));
        assert_eq!(thinking_budget(Effort::XHigh, s), Some(16_384));
        assert_eq!(thinking_budget(Effort::Max, s), Some(32_768));
    }

    #[test]
    fn reviewer_never_drops_below_high_budget() {
        let r = Some(ROLE_REVIEWER);
        assert_eq!(thinking_budget(Effort::Low, r), Some(16_384));
        assert_eq!(thinking_budget(Effort::Medium, r), Some(16_384));
        assert_eq!(thinking_budget(Effort::High, r), Some(16_384));
        assert_eq!(thinking_budget(Effort::XHigh, r), Some(32_768));
        assert_eq!(thinking_budget(Effort::Max, r), Some(65_536));
    }

    #[test]
    fn reasoning_effort_saturates_at_high() {
        assert_eq!(reasoning_effort_str(Effort::Low), "low");
        assert_eq!(reasoning_effort_str(Effort::Medium), "medium");
        assert_eq!(reasoning_effort_str(Effort::High), "high");
        assert_eq!(reasoning_effort_str(Effort::XHigh), "high");
        assert_eq!(reasoning_effort_str(Effort::Max), "high");
    }

    #[test]
    fn guidance_scales_posture_and_top_tiers_mandate_review() {
        assert!(guidance(Effort::Low).contains("favor speed"));
        assert!(guidance(Effort::High).contains("think before acting"));
        for e in [Effort::XHigh, Effort::Max] {
            let text = guidance(e).to_lowercase();
            assert!(text.contains("parallel"), "{e:?} should push parallelism");
            assert!(text.contains("review"), "{e:?} should require a reviewer");
        }
    }
}
