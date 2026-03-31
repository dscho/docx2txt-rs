/// Generate minimal .docx test fixtures that exercise docx2txt.
///
/// Usage: generate-fixtures [output-dir]
///        Defaults to tests/fixtures/ relative to CARGO_MANIFEST_DIR.

use std::path::PathBuf;
use std::{env, fs};

#[path = "../../tests/generated.rs"]
mod generated;

fn main() {
    let dir = match env::args().nth(1) {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
    };
    fs::create_dir_all(&dir).unwrap();

    for (name, bytes) in [
        ("basic-generated.docx", generated::basic_docx()),
        ("lists-generated.docx", generated::lists_docx()),
    ] {
        let path = dir.join(name);
        fs::write(&path, &bytes).unwrap();
        eprintln!("Created {} ({} bytes)", path.display(), bytes.len());
    }
}
