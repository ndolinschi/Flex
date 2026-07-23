use std::path::{Path, PathBuf};

use serde::Deserialize;

use agentloop_core::ProviderStreamEvent;
use agentloop_core::contracts::{MessageId, StopReason, ToolCallId};

use crate::mock_provider::{MOCK_MODEL, MockProvider, ScriptedTurn};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScenarioError {
    #[error(
        "cannot read scenario file {}: {source}. Check the path — committed scenarios live in \
         the testkit crate's `scenarios/` directory; build paths from CARGO_MANIFEST_DIR in tests.",
        path.display()
    )]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "scenario file {} is not valid scenario JSON: {source}. Expected \
         {{\"turns\": [{{\"events\": [{{\"text\": \"...\"}} | {{\"thinking\": \"...\"}} | \
         {{\"tool\": {{\"name\": \"...\", \"input\": {{...}}}}}}]}}]}}.",
        path.display()
    )]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Deserialize)]
struct ScenarioFile {
    turns: Vec<ScenarioTurn>,
}

#[derive(Debug, Deserialize)]
struct ScenarioTurn {
    events: Vec<ScenarioEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ScenarioEvent {
    Text(String),
    Thinking(String),
    Tool(ScenarioToolCall),
}

#[derive(Debug, Deserialize)]
struct ScenarioToolCall {
    name: String,
    #[serde(default = "empty_object")]
    input: serde_json::Value,
}

fn empty_object() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

pub fn scenario_turns(path: &Path) -> Result<Vec<ScriptedTurn>, ScenarioError> {
    let raw = std::fs::read_to_string(path).map_err(|source| ScenarioError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file: ScenarioFile = serde_json::from_str(&raw).map_err(|source| ScenarioError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(file.turns.into_iter().map(scripted_turn).collect())
}

fn scripted_turn(turn: ScenarioTurn) -> ScriptedTurn {
    let mut events = vec![ProviderStreamEvent::MessageStart {
        message_id: MessageId::generate(),
        model: MOCK_MODEL.to_owned(),
    }];
    let mut uses_tools = false;
    for event in turn.events {
        match event {
            ScenarioEvent::Text(text) => {
                events.push(ProviderStreamEvent::MarkdownDelta { text });
            }
            ScenarioEvent::Thinking(text) => {
                events.push(ProviderStreamEvent::ThinkingDelta { text });
            }
            ScenarioEvent::Tool(call) => {
                uses_tools = true;
                let call_id = ToolCallId::generate();
                events.push(ProviderStreamEvent::ToolCallStart {
                    call_id: call_id.clone(),
                    name: call.name,
                });
                events.push(ProviderStreamEvent::ToolCallArgsDelta {
                    call_id: call_id.clone(),
                    json_fragment: call.input.to_string(),
                });
                events.push(ProviderStreamEvent::ToolCallEnd { call_id });
            }
        }
    }
    events.push(ProviderStreamEvent::Usage(MockProvider::default_usage()));
    events.push(ProviderStreamEvent::MessageEnd {
        stop_reason: if uses_tools {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        },
    });
    Ok(events)
}

impl MockProvider {
    pub fn from_scenario_file(path: &Path) -> Result<Self, ScenarioError> {
        Ok(Self::with_turns(scenario_turns(path)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("scenarios")
            .join("tool_roundtrip.json")
    }

    fn tool_starts(events: &[ProviderStreamEvent]) -> Vec<(&ToolCallId, &str)> {
        events
            .iter()
            .filter_map(|event| match event {
                ProviderStreamEvent::ToolCallStart { call_id, name } => {
                    Some((call_id, name.as_str()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parses_the_committed_roundtrip_scenario() {
        let turns = scenario_turns(&roundtrip_path()).expect("committed scenario loads");
        assert_eq!(turns.len(), 2);

        let first = turns[0].as_ref().expect("scenario turns are Ok");
        assert!(matches!(
            &first[0],
            ProviderStreamEvent::MessageStart { model, .. } if model == MOCK_MODEL
        ));
        assert!(
            first
                .iter()
                .any(|event| matches!(event, ProviderStreamEvent::MarkdownDelta { .. })),
            "first turn keeps its markdown preamble"
        );
        let starts = tool_starts(first);
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].1, "echo");
        let args = first
            .iter()
            .find_map(|event| match event {
                ProviderStreamEvent::ToolCallArgsDelta { json_fragment, .. } => {
                    Some(json_fragment.as_str())
                }
                _ => None,
            })
            .expect("tool call has an args delta");
        let parsed: serde_json::Value = serde_json::from_str(args).expect("args are valid JSON");
        assert_eq!(parsed, serde_json::json!({"text": "ping"}));
        assert_eq!(
            first.last(),
            Some(&ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::ToolUse
            })
        );

        let second = turns[1].as_ref().expect("scenario turns are Ok");
        assert!(second.iter().any(|event| matches!(
            event,
            ProviderStreamEvent::MarkdownDelta { text } if text == "The echo tool returned: ping."
        )));
        assert_eq!(
            second.last(),
            Some(&ProviderStreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn
            })
        );
    }

    #[test]
    fn tool_call_ids_are_fresh_on_every_load() {
        let path = roundtrip_path();
        let first_load = scenario_turns(&path).expect("scenario loads");
        let second_load = scenario_turns(&path).expect("scenario loads");
        let first_events = first_load[0].as_ref().expect("turn is Ok");
        let second_events = second_load[0].as_ref().expect("turn is Ok");
        assert_ne!(
            tool_starts(first_events)[0].0,
            tool_starts(second_events)[0].0,
        );
    }

    #[tokio::test]
    async fn from_scenario_file_builds_a_playable_provider() {
        use agentloop_core::{ChatRequest, Provider};
        use futures::StreamExt;
        use tokio_util::sync::CancellationToken;

        let provider = MockProvider::from_scenario_file(&roundtrip_path()).expect("scenario loads");
        assert_eq!(provider.remaining_turns(), 2);

        let mut all_events = Vec::new();
        for _ in 0..2 {
            let stream = provider
                .stream_chat(
                    ChatRequest::new(MOCK_MODEL, Vec::new()),
                    CancellationToken::new(),
                )
                .await
                .expect("stream_chat succeeds");
            let events: Vec<_> = stream
                .map(|item| item.expect("scenario turns contain no Err items"))
                .collect()
                .await;
            all_events.push(events);
        }

        assert_eq!(tool_starts(&all_events[0]).len(), 1);
        assert!(all_events[1].iter().any(|event| matches!(
            event,
            ProviderStreamEvent::MarkdownDelta { text } if text.contains("ping")
        )));
        assert_eq!(provider.requests().len(), 2);
    }

    #[test]
    fn missing_file_is_an_io_error() {
        let path = roundtrip_path().with_file_name("does_not_exist.json");
        let err = scenario_turns(&path).expect_err("missing file must fail");
        assert!(matches!(err, ScenarioError::Io { .. }));
        assert!(err.to_string().contains("does_not_exist.json"));
    }

    #[test]
    fn invalid_json_is_a_parse_error_that_shows_the_expected_shape() {
        let path =
            std::env::temp_dir().join(format!("scenario-invalid-{}.json", MessageId::generate()));
        std::fs::write(&path, "{\"turns\": [{\"events\": [{\"beep\": true}]}]}")
            .expect("temp write succeeds");
        let err = scenario_turns(&path).expect_err("unknown event kind must fail");
        let _ = std::fs::remove_file(&path);

        assert!(matches!(err, ScenarioError::Parse { .. }));
        let message = err.to_string();
        assert!(
            message.contains("\"turns\""),
            "teaches the shape: {message}"
        );
    }
}
