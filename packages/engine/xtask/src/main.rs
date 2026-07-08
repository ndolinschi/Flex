//! Workspace automation. `cargo xtask schema` regenerates the committed JSON
//! Schemas under `schemas/v1/`; `cargo xtask schema --check` fails if the
//! committed files drift from the code (CI runs the check).

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::{Context, bail};
use schemars::{JsonSchema, schema_for};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("schema") => {
            let check = args.iter().any(|a| a == "--check");
            generate_schemas(check)
        }
        _ => {
            eprintln!("usage: cargo xtask schema [--check]");
            std::process::exit(2);
        }
    }
}

fn schema_dir() -> PathBuf {
    // xtask lives at <engine>/xtask; schemas at <engine>/schemas/v1.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("schemas")
        .join("v1")
}

fn generate_schemas(check: bool) -> anyhow::Result<()> {
    let targets = schema_targets();
    let dir = schema_dir();
    std::fs::create_dir_all(&dir).context("creating schema directory")?;

    let mut drifted = Vec::new();
    for (name, schema) in targets {
        let path = dir.join(format!("{name}.schema.json"));
        let rendered = serde_json::to_string_pretty(&schema)? + "\n";
        if check {
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            if existing != rendered {
                drifted.push(name);
            }
        } else {
            std::fs::write(&path, rendered)
                .with_context(|| format!("writing {}", path.display()))?;
            println!("wrote {}", path.display());
        }
    }

    if check && !drifted.is_empty() {
        bail!(
            "schema drift detected for: {}. Run `cargo xtask schema` and commit the result.",
            drifted.join(", ")
        );
    }
    Ok(())
}

fn schema_targets() -> Vec<(&'static str, schemars::Schema)> {
    fn entry<T: JsonSchema>(name: &'static str) -> (&'static str, schemars::Schema) {
        (name, schema_for!(T))
    }
    vec![
        entry::<agentloop_contracts::SessionEvent>("session_event"),
        entry::<agentloop_contracts::Hello>("hello"),
        entry::<agentloop_contracts::EngineError>("engine_error"),
        entry::<agentloop_contracts::SessionMeta>("session_meta"),
        entry::<agentloop_contracts::ToolCall>("tool_call"),
        entry::<agentloop_contracts::Transcript>("transcript"),
        entry::<agentloop_contracts::PromptInput>("prompt_input"),
        entry::<agentloop_contracts::NewSessionParams>("new_session_params"),
        entry::<agentloop_contracts::TurnOptions>("turn_options"),
    ]
}
