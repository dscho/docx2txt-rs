use std::fs;
use std::path::Path;
use std::process::Command;

fn fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn docx2txt_bin() -> std::path::PathBuf {
    let path = std::path::PathBuf::from(env!("CARGO_BIN_EXE_docx2txt"));
    // Ensure the binary exists (cargo test builds it for us via the above env var)
    assert!(path.exists(), "binary not found at {}", path.display());
    path
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

// -- CLI tests (file argument mode) --

#[test]
fn cli_stdin_to_stdout() {
    let input = fs::read(fixture_path("basic.docx")).unwrap();
    let expected = fs::read_to_string(fixture_path("basic.expected.txt")).unwrap();
    let output = Command::new(docx2txt_bin())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(&input).unwrap();
            child.wait_with_output()
        })
        .expect("failed to run docx2txt");
    assert!(output.status.success(), "exit code: {}", output.status);
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn cli_file_argument() {
    let tmp = std::env::temp_dir().join("docx2txt-test-cli-file.docx");
    let txt = tmp.with_extension("txt");

    // Clean up any leftovers from a previous run.
    let _ = fs::remove_file(&txt);

    fs::copy(fixture_path("basic.docx"), &tmp).unwrap();
    let output = Command::new(docx2txt_bin())
        .arg(&tmp)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("failed to run docx2txt");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(txt.exists(), "output file was not created");

    let expected = fs::read_to_string(fixture_path("basic.expected.txt")).unwrap();
    let actual = fs::read_to_string(&txt).unwrap();
    assert_eq!(actual, expected);

    // The status message goes to stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Text extracted from"), "missing status message: {}", stderr);

    fs::remove_file(&tmp).unwrap();
    fs::remove_file(&txt).unwrap();
}

#[test]
fn cli_file_argument_non_docx_extension() {
    let tmp = std::env::temp_dir().join("docx2txt-test-cli-nodocx.bin");
    let txt = std::env::temp_dir().join("docx2txt-test-cli-nodocx.bin.txt");

    let _ = fs::remove_file(&txt);

    fs::copy(fixture_path("basic.docx"), &tmp).unwrap();
    let output = Command::new(docx2txt_bin())
        .arg(&tmp)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("failed to run docx2txt");
    assert!(output.status.success());
    assert!(txt.exists(), ".bin.txt should have been created");

    fs::remove_file(&tmp).unwrap();
    fs::remove_file(&txt).unwrap();
}
