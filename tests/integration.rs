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

#[test]
fn basic_word_saved() {
    check_fixture("basic.docx", "basic.expected.txt");
}

#[test]
fn lists_word_saved() {
    check_fixture("lists.docx", "lists.expected.txt");
}
