use std::io::Cursor;

use docx_rs::{Docx, Paragraph, Run};

use crate::{ArtifactBuildSpec, ArtifactError, ArtifactKind, OfficeArtifact};

#[derive(Debug, Default)]
pub struct WordDocument;

impl OfficeArtifact for WordDocument {
    fn kind(&self) -> ArtifactKind {
        ArtifactKind::Document
    }

    fn extension(&self) -> &'static str {
        "docx"
    }

    fn capability_id(&self) -> &'static str {
        "CreateDocument"
    }

    fn build(&self, spec: &ArtifactBuildSpec) -> Result<Vec<u8>, ArtifactError> {
        let ArtifactBuildSpec::Document { title, body } = spec else {
            return Err(ArtifactError::WrongKind);
        };

        let mut docx = Docx::new();

        docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(title.as_str())));

        for chunk in body.split("\n\n") {
            let text = chunk.trim();
            if !text.is_empty() {
                docx = docx.add_paragraph(Paragraph::new().add_run(Run::new().add_text(text)));
            }
        }

        let mut buf = Cursor::new(Vec::new());
        docx.build()
            .pack(&mut buf)
            .map_err(|e| ArtifactError::Document(e.to_string()))?;

        Ok(buf.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_builds_non_empty_bytes() {
        let doc = WordDocument;
        let spec = ArtifactBuildSpec::Document {
            title: "Test Document".into(),
            body: "First paragraph.\n\nSecond paragraph.".into(),
        };
        let bytes = doc.build(&spec).expect("build");
        assert!(!bytes.is_empty(), "docx bytes must not be empty");
        assert_eq!(&bytes[..2], b"PK", "docx must start with ZIP magic");
    }

    #[test]
    fn word_rejects_wrong_kind() {
        let doc = WordDocument;
        let spec = ArtifactBuildSpec::Spreadsheet {
            title: "Sheet1".into(),
            headers: None,
            rows: vec![],
        };
        assert!(matches!(doc.build(&spec), Err(ArtifactError::WrongKind)));
    }
}
