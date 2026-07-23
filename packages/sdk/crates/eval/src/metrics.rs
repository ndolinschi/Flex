use serde::{Deserialize, Serialize};

use agentloop_contracts::{AgentEvent, TokenUsage, TurnSummary};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    pub turns: u32,

    pub usage: TokenUsage,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    pub num_model_calls: u32,
    pub num_tool_calls: u32,

    pub agent_duration_ms: u64,

    pub wall_ms: u64,
}

impl RunMetrics {
    pub fn fold_turn(&mut self, summary: &TurnSummary) {
        self.turns += 1;
        self.usage.add(&summary.usage);
        if let Some(cost) = summary.cost_usd {
            *self.cost_usd.get_or_insert(0.0) += cost;
        }
        self.num_model_calls += summary.num_model_calls;
        self.num_tool_calls += summary.num_tool_calls;
        self.agent_duration_ms += summary.duration_ms;
    }

    pub fn fold_event(&mut self, event: &AgentEvent) {
        if let AgentEvent::TurnCompleted { summary, .. } = event {
            self.fold_turn(summary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentloop_contracts::{TurnId, TurnStopReason};

    fn summary(input: u64, cost: Option<f64>) -> TurnSummary {
        TurnSummary {
            turn_id: TurnId::generate(),
            stop_reason: TurnStopReason::EndTurn,
            usage: TokenUsage {
                input,
                output: 5,
                ..TokenUsage::default()
            },
            cost_usd: cost,
            num_model_calls: 2,
            num_tool_calls: 3,
            duration_ms: 100,
        }
    }

    #[test]
    fn folds_turn_summaries() {
        let mut metrics = RunMetrics::default();
        metrics.fold_turn(&summary(10, Some(0.5)));
        metrics.fold_turn(&summary(20, None));
        assert_eq!(metrics.turns, 2);
        assert_eq!(metrics.usage.input, 30);
        assert_eq!(metrics.usage.output, 10);
        assert_eq!(metrics.cost_usd, Some(0.5));
        assert_eq!(metrics.num_model_calls, 4);
        assert_eq!(metrics.num_tool_calls, 6);
        assert_eq!(metrics.agent_duration_ms, 200);
    }

    #[test]
    fn fold_event_only_counts_turn_completed() {
        let mut metrics = RunMetrics::default();
        let turn_id = TurnId::generate();
        metrics.fold_event(&AgentEvent::TurnStarted {
            turn_id: turn_id.clone(),
        });
        assert_eq!(metrics.turns, 0);
        metrics.fold_event(&AgentEvent::TurnCompleted {
            turn_id,
            summary: summary(1, None),
        });
        assert_eq!(metrics.turns, 1);
    }
}
