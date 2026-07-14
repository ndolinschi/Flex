//! Fetch-and-convert body for the `scrape_page` tool.

use agentloop_contracts::{ToolOutput, ToolResultBlock};
use agentloop_core::{ToolContext, ToolError};
use reqwest::Client;
use schemars::JsonSchema;
use serde::Deserialize;

use super::body::{convert_page_body, render_scrape_output};

/// Default max output bytes/chars.
const DEFAULT_MAX_BYTES: usize = 200_000;

/// Hard cap on max_bytes input parameter.
const HARD_MAX_BYTES: usize = 1_000_000;

/// Raw HTML fetch limit — applies before conversion, large enough that we don't
/// cut HTML mid-tag.
const RAW_FETCH_LIMIT: usize = 4 * 1024 * 1024;

/// Maximum characters in the output fed back to the model.
const MAX_OUTPUT_CHARS: usize = 120_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScrapePageInput {
    /// HTTP or HTTPS URL of the page to scrape.
    pub(crate) url: String,
    /// Maximum response bytes to keep. Defaults to 200000, capped at 1000000.
    pub(crate) max_bytes: Option<usize>,
    /// Whether to include links found on the page in the output. Defaults to true.
    pub(crate) include_links: Option<bool>,
}

pub(crate) async fn scrape_page(
    client: &Client,
    ctx: ToolContext,
    input: serde_json::Value,
) -> Result<ToolOutput, ToolError> {
    let parsed: ScrapePageInput = serde_json::from_value(input).map_err(|err| {
        ToolError::InvalidInput(format!(
            "Input for `scrape_page` must be {{\"url\": \"https://...\", \"max_bytes\": \
             <optional number>}}: {err}."
        ))
    })?;

    let url = reqwest::Url::parse(&parsed.url).map_err(|err| {
        ToolError::InvalidInput(format!(
            "`url` is not a valid absolute URL for `scrape_page`: {err}. Pass an \
             http:// or https:// URL."
        ))
    })?;

    if !matches!(url.scheme(), "http" | "https") {
        return Err(ToolError::InvalidInput(format!(
            "`scrape_page` supports only http:// and https:// URLs, but got `{}`.",
            url.scheme()
        )));
    }

    let max_bytes = parsed
        .max_bytes
        .unwrap_or(DEFAULT_MAX_BYTES)
        .min(HARD_MAX_BYTES);
    let include_links = parsed.include_links.unwrap_or(true);

    let response = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
        result = client.get(url.clone()).send() => result.map_err(|err| {
            ToolError::Execution(format!(
                "`scrape_page` request to `{url}` failed: {err}."
            ))
        })?,
    };

    let status = response.status();
    let final_url = response.url().clone();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if !status.is_success() {
        return Err(ToolError::Execution(format!(
            "`scrape_page` returned HTTP {} for `{}`. Check the URL or fetch a more \
             specific page.",
            status.as_u16(),
            final_url
        )));
    }

    if response
        .content_length()
        .is_some_and(|len| len as usize > RAW_FETCH_LIMIT)
    {
        return Err(ToolError::Execution(format!(
            "`scrape_page` URL `{}` is too large (content-length exceeds {} bytes). \
             Use a more specific URL.",
            final_url, RAW_FETCH_LIMIT
        )));
    }

    let bytes = tokio::select! {
        _ = ctx.cancel.cancelled() => return Err(ToolError::Cancelled),
        result = response.bytes() => result.map_err(|err| {
            ToolError::Execution(format!(
                "`scrape_page` could not read `{final_url}`: {err}."
            ))
        })?,
    };

    let kept_len = bytes.len().min(RAW_FETCH_LIMIT);
    let raw_truncated = kept_len < bytes.len();
    let is_html = content_type
        .as_deref()
        .is_some_and(|ct| ct.contains("text/html") || ct.contains("application/xhtml"));

    let body = convert_page_body(
        &bytes,
        kept_len,
        is_html,
        max_bytes,
        include_links,
        final_url.as_str(),
    );
    let (rendered, output_truncated) = render_scrape_output(
        final_url.as_str(),
        status.as_u16(),
        &content_type,
        &body,
        MAX_OUTPUT_CHARS,
    );

    Ok(ToolOutput {
        content: vec![ToolResultBlock::markdown(rendered)],
        is_error: false,
        structured: Some(serde_json::json!({
            "url": final_url.as_str(),
            "status": status.as_u16(),
            "content_type": content_type,
            "bytes": bytes.len(),
            "kept_bytes": kept_len,
            "truncated": output_truncated || raw_truncated,
        })),
    })
}
