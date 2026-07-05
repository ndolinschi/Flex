//! Parsing of the Bedrock control-plane model catalog responses.
//!
//! `ListFoundationModels` gives directly-invokable on-demand models;
//! `ListInferenceProfiles` gives the cross-region profile ids (`us.…`, `eu.…`)
//! that newer models require. Both are filtered to usable, text-capable,
//! streaming entries and mapped onto [`ModelInfo`].

use agentloop_contracts::ModelInfo;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FoundationModelsResponse {
    #[serde(default, rename = "modelSummaries")]
    model_summaries: Vec<FoundationModelSummary>,
}

#[derive(Debug, Deserialize)]
struct FoundationModelSummary {
    #[serde(rename = "modelId")]
    model_id: String,
    #[serde(default, rename = "modelName")]
    model_name: Option<String>,
    #[serde(default, rename = "inputModalities")]
    input_modalities: Vec<String>,
    #[serde(default, rename = "outputModalities")]
    output_modalities: Vec<String>,
    #[serde(default, rename = "responseStreamingSupported")]
    response_streaming_supported: bool,
    #[serde(default, rename = "inferenceTypesSupported")]
    inference_types_supported: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct InferenceProfilesResponse {
    #[serde(default, rename = "inferenceProfileSummaries")]
    summaries: Vec<InferenceProfileSummary>,
}

#[derive(Debug, Deserialize)]
struct InferenceProfileSummary {
    #[serde(rename = "inferenceProfileId")]
    inference_profile_id: String,
    #[serde(default, rename = "inferenceProfileName")]
    inference_profile_name: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

/// Parse `ListFoundationModels`, keeping text-output, streaming, on-demand
/// models (the ones a bare `modelId` can invoke via Converse-stream).
pub(crate) fn parse_foundation_models(body: &str) -> Result<Vec<ModelInfo>, serde_json::Error> {
    let parsed: FoundationModelsResponse = serde_json::from_str(body)?;
    let models = parsed
        .model_summaries
        .into_iter()
        .filter(|m| {
            m.response_streaming_supported
                && contains_ci(&m.output_modalities, "TEXT")
                && contains_ci(&m.inference_types_supported, "ON_DEMAND")
        })
        .map(|m| ModelInfo {
            vision: contains_ci(&m.input_modalities, "IMAGE"),
            reasoning: false,
            context_window: None,
            display_name: m.model_name,
            id: m.model_id,
        })
        .collect();
    Ok(models)
}

/// Parse `ListInferenceProfiles`, keeping the ACTIVE profiles (directly
/// invokable cross-region ids).
pub(crate) fn parse_inference_profiles(body: &str) -> Result<Vec<ModelInfo>, serde_json::Error> {
    let parsed: InferenceProfilesResponse = serde_json::from_str(body)?;
    let models = parsed
        .summaries
        .into_iter()
        .filter(|p| {
            p.status
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("ACTIVE"))
                .unwrap_or(true)
        })
        .map(|p| ModelInfo {
            vision: false,
            reasoning: false,
            context_window: None,
            display_name: p.inference_profile_name,
            id: p.inference_profile_id,
        })
        .collect();
    Ok(models)
}

/// Merge model lists, de-duplicating by id (first occurrence wins), preserving
/// order. Used to combine foundation models with inference profiles.
pub(crate) fn merge_dedup(lists: impl IntoIterator<Item = Vec<ModelInfo>>) -> Vec<ModelInfo> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for list in lists {
        for model in list {
            if seen.insert(model.id.clone()) {
                out.push(model);
            }
        }
    }
    out
}

fn contains_ci(haystack: &[String], needle: &str) -> bool {
    haystack
        .iter()
        .any(|item| item.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_text_streaming_on_demand_models_only() {
        let body = r#"{
            "modelSummaries": [
                {"modelId":"anthropic.claude-3-5-sonnet-20241022-v2:0","modelName":"Claude 3.5 Sonnet v2",
                 "inputModalities":["TEXT","IMAGE"],"outputModalities":["TEXT"],
                 "responseStreamingSupported":true,"inferenceTypesSupported":["ON_DEMAND"]},
                {"modelId":"amazon.titan-embed-text-v2:0","outputModalities":["EMBEDDING"],
                 "responseStreamingSupported":false,"inferenceTypesSupported":["ON_DEMAND"]},
                {"modelId":"profile.only-model","outputModalities":["TEXT"],
                 "responseStreamingSupported":true,"inferenceTypesSupported":["INFERENCE_PROFILE"]}
            ]
        }"#;
        let models = parse_foundation_models(body).expect("parse");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "anthropic.claude-3-5-sonnet-20241022-v2:0");
        assert_eq!(
            models[0].display_name.as_deref(),
            Some("Claude 3.5 Sonnet v2")
        );
        assert!(models[0].vision, "IMAGE input → vision");
    }

    #[test]
    fn parses_active_inference_profiles() {
        let body = r#"{
            "inferenceProfileSummaries": [
                {"inferenceProfileId":"us.anthropic.claude-3-7-sonnet-20250219-v1:0",
                 "inferenceProfileName":"US Claude 3.7 Sonnet","status":"ACTIVE"},
                {"inferenceProfileId":"eu.retired","status":"INACTIVE"}
            ]
        }"#;
        let models = parse_inference_profiles(body).expect("parse");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "us.anthropic.claude-3-7-sonnet-20250219-v1:0");
    }

    #[test]
    fn merge_dedup_keeps_first_by_id() {
        let a = vec![ModelInfo {
            id: "x".to_owned(),
            display_name: Some("A".to_owned()),
            context_window: None,
            reasoning: false,
            vision: false,
        }];
        let b = vec![
            ModelInfo {
                id: "x".to_owned(),
                display_name: Some("B".to_owned()),
                context_window: None,
                reasoning: false,
                vision: false,
            },
            ModelInfo {
                id: "y".to_owned(),
                display_name: None,
                context_window: None,
                reasoning: false,
                vision: false,
            },
        ];
        let merged = merge_dedup([a, b]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].display_name.as_deref(), Some("A")); // first wins
        assert_eq!(merged[1].id, "y");
    }

    #[test]
    fn tolerates_empty_or_missing_arrays() {
        assert!(parse_foundation_models("{}").expect("parse").is_empty());
        assert!(parse_inference_profiles("{}").expect("parse").is_empty());
    }
}
