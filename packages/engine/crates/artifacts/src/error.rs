//! Error type for the artifacts plugin.

/// Errors that can occur when building or writing an office artifact.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArtifactError {
    /// The `ArtifactBuildSpec` variant does not match the `OfficeArtifact` implementation.
    #[error("wrong spec kind for this artifact builder")]
    WrongKind,

    /// A Word document build failure from docx-rs.
    #[error("document build error: {0}")]
    Document(String),

    /// A spreadsheet build failure from rust_xlsxwriter.
    #[error("spreadsheet build error: {0}")]
    Spreadsheet(String),

    /// A presentation build or archive failure.
    #[error("presentation build error: {0}")]
    Presentation(String),

    /// Underlying I/O error (file write, dir creation, etc.).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
