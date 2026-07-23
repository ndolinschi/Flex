use std::io::{Cursor, Write};

use zip::CompressionMethod;
use zip::write::FileOptions;

use crate::{ArtifactBuildSpec, ArtifactError, ArtifactKind, OfficeArtifact, Slide};

#[derive(Debug, Default)]
pub struct PresentationDocument;

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn content_types_xml(slide_count: usize) -> String {
    let mut overrides = String::new();
    for i in 1..=slide_count {
        overrides.push_str(&format!(
            r#"  <Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#,
        ));
        overrides.push('\n');
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
  <Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
  <Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
{overrides}</Types>"#
    )
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#;

fn presentation_xml(slide_count: usize) -> String {
    let mut sld_ids = String::new();
    for i in 0..slide_count {
        let sld_id = 256u32 + i as u32;
        let rid = i + 2;
        sld_ids.push_str(&format!(r#"    <p:sldId id="{sld_id}" r:id="rId{rid}"/>"#,));
        sld_ids.push('\n');
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
                xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
                xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                saveSubsetFonts="1">
  <p:sldMasterIdLst>
    <p:sldMasterId id="2147483648" r:id="rId1"/>
  </p:sldMasterIdLst>
  <p:sldIdLst>
{sld_ids}  </p:sldIdLst>
  <p:sldSz cx="9144000" cy="6858000" type="screen4x3"/>
  <p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
    )
}

fn presentation_rels_xml(slide_count: usize) -> String {
    let mut slide_rels = String::new();
    for i in 0..slide_count {
        let rid = i + 2;
        slide_rels.push_str(&format!(
            r#"  <Relationship Id="rId{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{n}.xml"/>"#,
            n = i + 1,
        ));
        slide_rels.push('\n');
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
{slide_rels}</Relationships>"#
    )
}

const SLIDE_MASTER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
             xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
             xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
    </p:spTree>
  </p:cSld>
  <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
  <p:sldLayoutIdLst>
    <p:sldLayoutId id="2147483649" r:id="rId1"/>
  </p:sldLayoutIdLst>
</p:sldMaster>"#;

const SLIDE_MASTER_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#;

const SLIDE_LAYOUT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
             xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
             xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
             type="obj" preserve="1">
  <p:cSld name="Content">
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
    </p:spTree>
  </p:cSld>
</p:sldLayout>"#;

const SLIDE_LAYOUT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#;

const SLIDE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#;

fn slide_xml(slide: &Slide) -> String {
    let title = xml_escape(&slide.title);

    let mut bullet_paras = String::new();
    if slide.bullets.is_empty() {
        bullet_paras.push_str("          <a:p/>\n");
    } else {
        for bullet in &slide.bullets {
            let text = xml_escape(bullet);
            bullet_paras.push_str(&format!(
                "          <a:p><a:r><a:t>{text}</a:t></a:r></a:p>\n",
            ));
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld>
    <p:spTree>
      <p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
      <p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="2" name="Title 1"/>
          <p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>
          <p:nvPr><p:ph type="title"/></p:nvPr>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm><a:off x="457200" y="274638"/><a:ext cx="8229600" cy="1143000"/></a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:bodyPr/><a:lstStyle/>
          <a:p><a:r><a:t>{title}</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
      <p:sp>
        <p:nvSpPr>
          <p:cNvPr id="3" name="Content Placeholder 2"/>
          <p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>
          <p:nvPr><p:ph idx="1"/></p:nvPr>
        </p:nvSpPr>
        <p:spPr>
          <a:xfrm><a:off x="457200" y="1600200"/><a:ext cx="8229600" cy="4525963"/></a:xfrm>
        </p:spPr>
        <p:txBody>
          <a:bodyPr/><a:lstStyle/>
{bullet_paras}        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
  <p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>"#
    )
}

fn write_entry(
    zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    content: &[u8],
) -> Result<(), ArtifactError> {
    let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(name, options)
        .map_err(|e| ArtifactError::Presentation(e.to_string()))?;
    zip.write_all(content)
        .map_err(|e| ArtifactError::Presentation(e.to_string()))?;
    Ok(())
}

impl OfficeArtifact for PresentationDocument {
    fn kind(&self) -> ArtifactKind {
        ArtifactKind::Presentation
    }

    fn extension(&self) -> &'static str {
        "pptx"
    }

    fn capability_id(&self) -> &'static str {
        "CreatePresentation"
    }

    fn build(&self, spec: &ArtifactBuildSpec) -> Result<Vec<u8>, ArtifactError> {
        let ArtifactBuildSpec::Presentation { slides, .. } = spec else {
            return Err(ArtifactError::WrongKind);
        };

        let slide_count = slides.len().max(1);
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);

        write_entry(
            &mut zip,
            "[Content_Types].xml",
            content_types_xml(slide_count).as_bytes(),
        )?;
        write_entry(&mut zip, "_rels/.rels", ROOT_RELS.as_bytes())?;
        write_entry(
            &mut zip,
            "ppt/presentation.xml",
            presentation_xml(slide_count).as_bytes(),
        )?;
        write_entry(
            &mut zip,
            "ppt/_rels/presentation.xml.rels",
            presentation_rels_xml(slide_count).as_bytes(),
        )?;
        write_entry(
            &mut zip,
            "ppt/slideMasters/slideMaster1.xml",
            SLIDE_MASTER_XML.as_bytes(),
        )?;
        write_entry(
            &mut zip,
            "ppt/slideMasters/_rels/slideMaster1.xml.rels",
            SLIDE_MASTER_RELS.as_bytes(),
        )?;
        write_entry(
            &mut zip,
            "ppt/slideLayouts/slideLayout1.xml",
            SLIDE_LAYOUT_XML.as_bytes(),
        )?;
        write_entry(
            &mut zip,
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
            SLIDE_LAYOUT_RELS.as_bytes(),
        )?;

        let effective_slides: Vec<&Slide>;
        let placeholder_slide;
        let slides_to_write: &[&Slide] = if slides.is_empty() {
            placeholder_slide = Slide {
                title: String::new(),
                bullets: Vec::new(),
            };
            effective_slides = vec![&placeholder_slide];
            &effective_slides
        } else {
            effective_slides = slides.iter().collect();
            &effective_slides
        };

        for (i, slide) in slides_to_write.iter().enumerate() {
            let n = i + 1;
            write_entry(
                &mut zip,
                &format!("ppt/slides/slide{n}.xml"),
                slide_xml(slide).as_bytes(),
            )?;
            write_entry(
                &mut zip,
                &format!("ppt/slides/_rels/slide{n}.xml.rels"),
                SLIDE_RELS.as_bytes(),
            )?;
        }

        let cursor = zip
            .finish()
            .map_err(|e| ArtifactError::Presentation(e.to_string()))?;

        Ok(cursor.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_builds_non_empty_zip() {
        let deck = PresentationDocument;
        let spec = ArtifactBuildSpec::Presentation {
            title: "My Deck".into(),
            slides: vec![
                Slide {
                    title: "Slide One".into(),
                    bullets: vec!["First point".into(), "Second point".into()],
                },
                Slide {
                    title: "Slide Two".into(),
                    bullets: vec!["Alpha".into()],
                },
            ],
        };
        let bytes = deck.build(&spec).expect("build");
        assert!(!bytes.is_empty(), "pptx bytes must not be empty");
        assert_eq!(&bytes[..2], b"PK", "pptx must start with ZIP magic");
    }

    #[test]
    fn presentation_with_special_chars_is_safe() {
        let deck = PresentationDocument;
        let spec = ArtifactBuildSpec::Presentation {
            title: "Test".into(),
            slides: vec![Slide {
                title: "Title with <tags> & \"quotes\"".into(),
                bullets: vec!["Bullet & more".into()],
            }],
        };
        let bytes = deck.build(&spec).expect("build");
        assert_eq!(&bytes[..2], b"PK");
    }

    #[test]
    fn presentation_rejects_wrong_kind() {
        let deck = PresentationDocument;
        let spec = ArtifactBuildSpec::Document {
            title: "Doc".into(),
            body: "body".into(),
        };
        assert!(matches!(deck.build(&spec), Err(ArtifactError::WrongKind)));
    }

    #[test]
    fn xml_escape_handles_all_special_chars() {
        assert_eq!(
            xml_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
        assert_eq!(xml_escape("plain text"), "plain text");
    }
}
