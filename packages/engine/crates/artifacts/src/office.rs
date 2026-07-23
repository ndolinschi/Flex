use serde::{Deserialize, Serialize};

use crate::{ArtifactError, ArtifactKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub title: String,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ArtifactBuildSpec {
    Document {
        title: String,
        body: String,
    },
    Spreadsheet {
        title: String,
        headers: Option<Vec<String>>,
        rows: Vec<Vec<String>>,
    },
    Presentation {
        title: String,
        slides: Vec<Slide>,
    },
}

pub trait OfficeArtifact: Send + Sync {
    fn kind(&self) -> ArtifactKind;

    fn extension(&self) -> &'static str;

    fn capability_id(&self) -> &'static str;

    fn build(&self, spec: &ArtifactBuildSpec) -> Result<Vec<u8>, ArtifactError>;
}
