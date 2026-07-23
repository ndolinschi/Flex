use std::sync::Arc;

use crate::OfficeArtifact;

#[derive(Debug, Clone)]
pub struct ArtifactMcpCapability {
    pub name: &'static str,
    pub extension: &'static str,
    pub description: &'static str,
}

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
