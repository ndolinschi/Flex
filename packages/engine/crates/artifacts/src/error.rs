#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArtifactError {
    #[error("wrong spec kind for this artifact builder")]
    WrongKind,

    #[error("document build error: {0}")]
    Document(String),

    #[error("spreadsheet build error: {0}")]
    Spreadsheet(String),

    #[error("presentation build error: {0}")]
    Presentation(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
