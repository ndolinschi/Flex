use super::*;

#[test]
fn extract_content_core_drops_boilerplate() {
    let input = "\
# Nav\nHome | About | Contact\n\n\
# Footer\nCopyright 2025\n\n\
# Main Article\n\
This is the first paragraph of the main article.\n\
\n\
This is the second paragraph of the main article.\n\
\n\
This is the third paragraph with important details.";

    let core = extract_content_core(input);

    assert!(core.contains("Main Article"));
    assert!(core.contains("first paragraph"));
    assert!(core.contains("third paragraph"));

    assert!(!core.contains("Copyright 2025"));
    assert!(!core.contains("Home | About"));
}

#[test]
fn extract_content_core_full_when_too_small() {
    let input = "Single paragraph only.\n\nShort second paragraph.";
    let core = extract_content_core(input);
    assert_eq!(core, input);
}

#[test]
fn extract_content_core_single_section() {
    let input = "\
Introduction paragraph with some context.\n\n\
Main body paragraph with the bulk of the content here.\n\n\
Conclusion paragraph wrapping everything up nicely.";
    let core = extract_content_core(input);
    assert!(core.contains("Introduction"));
    assert!(core.contains("Main body"));
    assert!(core.contains("Conclusion"));
}
