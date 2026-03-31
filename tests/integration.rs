use std::fs;
use std::path::Path;

fn fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn check_fixture(docx_name: &str, expected_name: &str) {
    let input = fs::read(fixture_path(docx_name))
        .unwrap_or_else(|e| panic!("failed to read {}: {}", docx_name, e));
    let expected = fs::read_to_string(fixture_path(expected_name))
        .unwrap_or_else(|e| panic!("failed to read {}: {}", expected_name, e));
    let actual = docx2txt::convert(&input)
        .unwrap_or_else(|e| panic!("convert({}) failed: {}", docx_name, e));
    assert_eq!(actual, expected, "output mismatch for {}", docx_name);
}

fn check_bytes(docx_bytes: &[u8], expected: &str, label: &str) {
    let actual = docx2txt::convert(docx_bytes)
        .unwrap_or_else(|e| panic!("convert({}) failed: {}", label, e));
    assert_eq!(actual, expected, "output mismatch for {}", label);
}

mod generated;

// -- Word-saved fixtures (committed as binary files) --

#[test]
fn basic_word_saved() {
    check_fixture("basic.docx", "basic.expected.txt");
}

#[test]
fn lists_word_saved() {
    check_fixture("lists.docx", "lists.expected.txt");
}

// -- Generated fixtures (built in-memory at test time) --

#[test]
fn basic_generated() {
    let docx = generated::basic_docx();
    let expected = fs::read_to_string(fixture_path("basic-generated.expected.txt"))
        .expect("missing basic-generated.expected.txt");
    check_bytes(&docx, &expected, "basic-generated");
}

#[test]
fn lists_generated() {
    let docx = generated::lists_docx();
    let expected = fs::read_to_string(fixture_path("lists-generated.expected.txt"))
        .expect("missing lists-generated.expected.txt");
    check_bytes(&docx, &expected, "lists-generated");
}
