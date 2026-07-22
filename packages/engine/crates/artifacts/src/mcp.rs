//! MCP-oriented capability surface for [`OfficeArtifact`] backends.
//!
//! Native tools (`CreateDocument` / `CreateSpreadsheet` / `CreatePresentation`)
//! already expose each backend to the agent loop. This module documents how the
//! same trait maps to an MCP tool catalog so an external MCP server (or a
//! future in-process bridge) can wrap backends without touching the loop.
//!
//! Each [`OfficeArtifact::capability_id`] is the stable MCP tool name. Input
//! schemas mirror the native tools; generation still goes through
//! [`OfficeArtifact::build`].

use std::sync::Arc;

use crate::OfficeArtifact;

/// One MCP-shaped capability descriptor derived from an [`OfficeArtifact`].
#[derive(Debug, Clone)]
pub struct ArtifactMcpCapability {
    /// MCP / native tool name (`CreateDocument`, …).
    pub name: &'static str,
    /// File extension without a leading dot.
    pub extension: &'static str,
    /// Human-readable summary for MCP `tools/list`.
    pub description: &'static str,
}

/// Describe the three standard office backends as MCP capabilities.
///
/// Callers that host an MCP server can register one tool per capability and
/// forward arguments into the matching [`OfficeArtifact`] from
/// [`ArtifactsPlugin::with_backends`](crate::ArtifactsPlugin::with_backends).
pub fn mcp_capabilities(
    word: &Arc<dyn OfficeArtifact>,
    sheet: &Arc<dyn OfficeArtifact>,
    deck: &Arc<dyn OfficeArtifact>,
) -> [ArtifactMcpCapability; 3] {
    [
        ArtifactMcpCapability {
            name: word.capability_id(),
            extension: word.extension(),
            description: "Create a Word (.docx) document from a title and body paragraphs.",
        },
        ArtifactMcpCapability {
            name: sheet.capability_id(),
            extension: sheet.extension(),
            description: "Create an Excel (.xlsx) spreadsheet from headers and rows.",
        },
        ArtifactMcpCapability {
            name: deck.capability_id(),
            extension: deck.extension(),
            description: "Create a PowerPoint (.pptx) presentation from title/bullet slides.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PresentationDocument, SpreadsheetDocument, WordDocument};

    #[test]
    fn capabilities_match_backend_ids() {
        let word: Arc<dyn OfficeArtifact> = Arc::new(WordDocument);
        let sheet: Arc<dyn OfficeArtifact> = Arc::new(SpreadsheetDocument);
        let deck: Arc<dyn OfficeArtifact> = Arc::new(PresentationDocument);
        let caps = mcp_capabilities(&word, &sheet, &deck);
        assert_eq!(caps[0].name, "CreateDocument");
        assert_eq!(caps[1].name, "CreateSpreadsheet");
        assert_eq!(caps[2].name, "CreatePresentation");
        assert_eq!(caps[0].extension, "docx");
        assert_eq!(caps[1].extension, "xlsx");
        assert_eq!(caps[2].extension, "pptx");
    }
}
