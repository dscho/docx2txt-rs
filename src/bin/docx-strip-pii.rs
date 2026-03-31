/// Strip PII from a .docx file.
///
/// Reads a .docx from the path given as the first argument, removes
/// metadata parts (docProps/, docMetadata/) and scrubs author names,
/// GUIDs, and timestamps from the remaining XML, then writes the
/// cleaned archive to stdout.
///
/// Usage: docx-strip-pii input.docx > cleaned.docx

use std::io::{self, Cursor, Read, Write};

use regex::Regex;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Parts that carry PII and are not needed for text extraction.
const STRIP_PARTS: &[&str] = &[
    "docProps/core.xml",
    "docProps/app.xml",
    "docMetadata/LabelInfo.xml",
];

/// Scrub PII from XML content that we do keep.
fn scrub_xml(xml: &str) -> String {
    // Remove <cp:lastModifiedBy>...</cp:lastModifiedBy>
    let re = Regex::new(r"<cp:lastModifiedBy>[^<]*</cp:lastModifiedBy>").unwrap();
    let xml = re.replace_all(xml, "").to_string();

    // Remove <dc:creator>...</dc:creator>
    let re = Regex::new(r"<dc:creator>[^<]*</dc:creator>").unwrap();
    let xml = re.replace_all(&xml, "").to_string();

    // Neutralize w:author="..." attributes (e.g. on <w:del>)
    let re = Regex::new(r#"w:author="[^"]*""#).unwrap();
    let xml = re.replace_all(&xml, r#"w:author="""#).to_string();

    // Zero out w15:val="{GUID}" persistent document IDs
    let re = Regex::new(r#"w15:val="\{[0-9A-Fa-f-]+\}""#).unwrap();
    let xml = re.replace_all(
        &xml,
        r#"w15:val="{00000000-0000-0000-0000-000000000000}""#,
    )
    .to_string();

    // Zero out MIP label siteId GUIDs
    let re = Regex::new(r#"siteId="\{[0-9A-Fa-f-]+\}""#).unwrap();
    let xml = re.replace_all(
        &xml,
        r#"siteId="{00000000-0000-0000-0000-000000000000}""#,
    )
    .to_string();

    // Clamp timestamps to epoch
    let re = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z").unwrap();
    let xml = re
        .replace_all(&xml, "1970-01-01T00:00:00Z")
        .to_string();

    xml
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: docx-strip-pii input.docx > cleaned.docx")?;

    let data = std::fs::read(&path)?;
    let mut archive = ZipArchive::new(Cursor::new(&data))?;
    let mut buf = Cursor::new(Vec::new());

    {
        let mut writer = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();

            if STRIP_PARTS.iter().any(|s| name == *s) {
                continue;
            }

            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;

            let final_content =
                if name.ends_with(".xml") || name.ends_with(".rels") {
                    let xml = String::from_utf8(content)?;
                    scrub_xml(&xml).into_bytes()
                } else {
                    content
                };

            writer.start_file(&name, opts)?;
            writer.write_all(&final_content)?;
        }

        writer.finish()?;
    }

    io::stdout().write_all(buf.get_ref())?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("docx-strip-pii: {}", e);
        std::process::exit(1);
    }
}
