use rust_xlsxwriter::Workbook;

use crate::{ArtifactBuildSpec, ArtifactError, ArtifactKind, OfficeArtifact};

#[derive(Debug, Default)]
pub struct SpreadsheetDocument;

impl OfficeArtifact for SpreadsheetDocument {
    fn kind(&self) -> ArtifactKind {
        ArtifactKind::Spreadsheet
    }

    fn extension(&self) -> &'static str {
        "xlsx"
    }

    fn capability_id(&self) -> &'static str {
        "CreateSpreadsheet"
    }

    fn build(&self, spec: &ArtifactBuildSpec) -> Result<Vec<u8>, ArtifactError> {
        let ArtifactBuildSpec::Spreadsheet {
            title,
            headers,
            rows,
        } = spec
        else {
            return Err(ArtifactError::WrongKind);
        };

        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet();

        worksheet
            .set_name(title.as_str())
            .map_err(|e| ArtifactError::Spreadsheet(e.to_string()))?;

        let mut next_row: u32 = 0;

        if let Some(hdrs) = headers {
            for (col, header) in hdrs.iter().enumerate() {
                worksheet
                    .write_string(next_row, col as u16, header.as_str())
                    .map_err(|e| ArtifactError::Spreadsheet(e.to_string()))?;
            }
            next_row += 1;
        }

        for row_data in rows {
            for (col, cell) in row_data.iter().enumerate() {
                worksheet
                    .write_string(next_row, col as u16, cell.as_str())
                    .map_err(|e| ArtifactError::Spreadsheet(e.to_string()))?;
            }
            next_row += 1;
        }

        let bytes = workbook
            .save_to_buffer()
            .map_err(|e| ArtifactError::Spreadsheet(e.to_string()))?;

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spreadsheet_builds_non_empty_bytes() {
        let sheet = SpreadsheetDocument;
        let spec = ArtifactBuildSpec::Spreadsheet {
            title: "Report".into(),
            headers: Some(vec!["Name".into(), "Value".into()]),
            rows: vec![
                vec!["alpha".into(), "1".into()],
                vec!["beta".into(), "2".into()],
            ],
        };
        let bytes = sheet.build(&spec).expect("build");
        assert!(!bytes.is_empty(), "xlsx bytes must not be empty");
        assert_eq!(&bytes[..2], b"PK", "xlsx must start with ZIP magic");
    }

    #[test]
    fn spreadsheet_rejects_wrong_kind() {
        let sheet = SpreadsheetDocument;
        let spec = ArtifactBuildSpec::Document {
            title: "Doc".into(),
            body: "body".into(),
        };
        assert!(matches!(sheet.build(&spec), Err(ArtifactError::WrongKind)));
    }
}
