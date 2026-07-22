//! `OfficeArtifact` trait and `ArtifactBuildSpec` — the generation contract.
//!
//! Keeping generation out of the loop crate: only tools call `build()`.

use serde::{Deserialize, Serialize};

use crate::{ArtifactError, ArtifactKind};

/// A single presentation slide.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    /// Slide title text.
    pub title: String,
    /// Bullet-point lines for the slide body.
    pub bullets: Vec<String>,
}

/// Kind-specific input data for an office artifact builder.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ArtifactBuildSpec {
    /// Input for a Word document (.docx).
    Document {
        /// Document title (used as the first bold paragraph).
        title: String,
        /// Body text: blank-line-separated paragraphs.
        body: String,
    },
    /// Input for an Excel spreadsheet (.xlsx).
    Spreadsheet {
        /// Sheet tab name.
        title: String,
        /// Optional header row.
        headers: Option<Vec<String>>,
        /// Data rows (strings; rows can have varying column counts).
        rows: Vec<Vec<String>>,
    },
    /// Input for a PowerPoint presentation (.pptx).
    Presentation {
        /// Presentation title (not currently embedded; kept for future use).
        title: String,
        /// Ordered slide list.
        slides: Vec<Slide>,
    },
}

/// An office artifact generator.
///
/// Implementations are cheap to share across threads (`Arc<dyn OfficeArtifact>`).
/// Generation is synchronous and CPU-bound; callers that need async should spawn
/// a blocking task.
pub trait OfficeArtifact: Send + Sync {
    /// Which kind of artifact this builder produces.
    fn kind(&self) -> ArtifactKind;

    /// File extension without a leading dot: `"docx"`, `"xlsx"`, or `"pptx"`.
    fn extension(&self) -> &'static str;

    /// Stable tool / MCP capability id, e.g. `"CreateDocument"`.
    fn capability_id(&self) -> &'static str;

    /// Build and return the raw artifact bytes.
    ///
    /// Returns `Err(ArtifactError::WrongKind)` when `spec` is not the variant
    /// that corresponds to this builder's [`kind`](Self::kind).
    fn build(&self, spec: &ArtifactBuildSpec) -> Result<Vec<u8>, ArtifactError>;
}
