pub mod error;
pub mod kind;
pub mod mcp;
pub mod office;
pub mod path;
pub mod presentation;
pub mod spreadsheet;
pub mod tools;
pub mod word;

pub use error::ArtifactError;
pub use kind::ArtifactKind;
pub use mcp::{ArtifactMcpCapability, mcp_capabilities};
pub use office::{ArtifactBuildSpec, OfficeArtifact, Slide};
pub use presentation::PresentationDocument;
pub use spreadsheet::SpreadsheetDocument;
pub use word::WordDocument;

use std::sync::Arc;

use agentloop_core::{Plugin, Tool};

use tools::document::CreateDocumentTool;
use tools::presentation::CreatePresentationTool;
use tools::spreadsheet::CreateSpreadsheetTool;

pub struct ArtifactsPlugin {
    word: Arc<dyn OfficeArtifact>,
    sheet: Arc<dyn OfficeArtifact>,
    deck: Arc<dyn OfficeArtifact>,
}

impl ArtifactsPlugin {
    pub fn with_backends(
        word: Arc<dyn OfficeArtifact>,
        sheet: Arc<dyn OfficeArtifact>,
        deck: Arc<dyn OfficeArtifact>,
    ) -> Self {
        Self { word, sheet, deck }
    }
}

impl Default for ArtifactsPlugin {
    fn default() -> Self {
        Self {
            word: Arc::new(WordDocument),
            sheet: Arc::new(SpreadsheetDocument),
            deck: Arc::new(PresentationDocument),
        }
    }
}

impl Plugin for ArtifactsPlugin {
    fn id(&self) -> &'static str {
        "artifacts"
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        vec![
            Arc::new(CreateDocumentTool::new(Arc::clone(&self.word))),
            Arc::new(CreateSpreadsheetTool::new(Arc::clone(&self.sheet))),
            Arc::new(CreatePresentationTool::new(Arc::clone(&self.deck))),
        ]
    }

    fn system_prompt_fragment(&self) -> Option<String> {
        Some(
            "# Office artifacts\n\
             Use `CreateDocument` for `.docx`, `CreateSpreadsheet` for `.xlsx`, and \
             `CreatePresentation` for `.pptx` files. Place generated files under \
             `artifacts/` or `reports/` relative to the project root unless the user \
             specifies a different location. \
             Never use the `Write` tool for these file types — it writes raw text and \
             produces corrupt binary files that Office applications cannot open."
                .to_owned(),
        )
    }
}

#[cfg(test)]
mod integration_tests {
    use std::path::PathBuf;

    use agentloop_contracts::{SessionId, ToolCallId, TurnId};
    use agentloop_core::{EventSink, ToolContext};
    use tokio_util::sync::CancellationToken;

    use super::*;

    fn ctx(cwd: PathBuf) -> ToolContext {
        let (events, _rx) = EventSink::channel();
        ToolContext {
            session_id: SessionId::from("sess-artifacts-test"),
            turn_id: TurnId::from("turn-1"),
            call_id: ToolCallId::from("call-1"),
            cwd,
            cancel: CancellationToken::new(),
            events,
        }
    }

    #[tokio::test]
    async fn create_document_writes_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cwd = dir.path().to_path_buf();
        let file_path = cwd.join("out.docx");

        let plugin = ArtifactsPlugin::default();
        let tools = plugin.tools();
        let tool = tools
            .iter()
            .find(|t| t.descriptor().name == "CreateDocument")
            .expect("tool");

        let input = serde_json::json!({
            "file_path": file_path.display().to_string(),
            "title": "Test",
            "body": "Hello world.\n\nSecond paragraph."
        });
        let result = tool.run(ctx(cwd), input).await.expect("run");
        assert!(!result.is_error, "should not be an error");
        assert!(file_path.exists(), "file must be written");
        let bytes = std::fs::read(&file_path).expect("read");
        assert!(!bytes.is_empty(), "file must have content");
        assert_eq!(&bytes[..2], b"PK", "docx must be a ZIP");
    }
}
