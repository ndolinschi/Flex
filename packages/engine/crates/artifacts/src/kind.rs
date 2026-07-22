//! `ArtifactKind` — the three supported Office document types.

/// Discriminant for the three supported Office artifact types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ArtifactKind {
    /// A Word document (.docx).
    Document,
    /// An Excel spreadsheet (.xlsx).
    Spreadsheet,
    /// A PowerPoint presentation (.pptx).
    Presentation,
}

impl std::fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document => f.write_str("document"),
            Self::Spreadsheet => f.write_str("spreadsheet"),
            Self::Presentation => f.write_str("presentation"),
        }
    }
}
