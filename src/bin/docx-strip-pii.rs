/// Strip PII from a .docx file.
///
/// Reads a .docx from the path given as the first argument, removes
/// metadata parts (docProps/, docMetadata/) and scrubs author names,
/// GUIDs, and timestamps from the remaining XML, then writes the
/// cleaned archive to stdout.
///
/// Usage: docx-strip-pii input.docx > cleaned.docx

use std::io::{self, Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

/// Parts that carry PII and are not needed for text extraction.
const STRIP_PARTS: &[&str] = &[
    "docProps/core.xml",
    "docProps/app.xml",
    "docMetadata/LabelInfo.xml",
];

/// Remove `<open>...</close>` from xml.
fn remove_element(xml: &mut String, open: &str, close: &str) {
    while let Some(start) = xml.find(open) {
        if let Some(end_rel) = xml[start..].find(close) {
            xml.replace_range(start..start + end_rel + close.len(), "");
        } else {
            break;
        }
    }
}

/// Replace `attr="value"` with `attr="replacement"` wherever attr appears.
fn replace_attr_value(xml: &mut String, attr: &str, replacement: &str) {
    let needle = format!("{attr}=\"");
    let mut pos = 0;
    while let Some(rel) = xml[pos..].find(&needle) {
        let val_start = pos + rel + needle.len();
        if let Some(val_end_rel) = xml[val_start..].find('"') {
            let val_end = val_start + val_end_rel;
            xml.replace_range(val_start..val_end, replacement);
            pos = val_start + replacement.len() + 1;
        } else {
            break;
        }
    }
}

/// Replace ISO 8601 timestamps (YYYY-MM-DDTHH:MM:SSZ) with epoch.
fn replace_timestamps(xml: &mut String) {
    // Timestamps appear inside XML attributes/elements with a fixed format.
    let mut pos = 0;
    while pos + 20 <= xml.len() {
        let s = &xml[pos..];
        // Look for the pattern: DDDD-DD-DDTDD:DD:DDZ (20 chars)
        if s.len() >= 20
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && s.as_bytes()[10] == b'T'
            && s.as_bytes()[13] == b':'
            && s.as_bytes()[16] == b':'
            && s.as_bytes()[19] == b'Z'
            && s[..4].bytes().all(|b| b.is_ascii_digit())
            && s[5..7].bytes().all(|b| b.is_ascii_digit())
            && s[8..10].bytes().all(|b| b.is_ascii_digit())
            && s[11..13].bytes().all(|b| b.is_ascii_digit())
            && s[14..16].bytes().all(|b| b.is_ascii_digit())
            && s[17..19].bytes().all(|b| b.is_ascii_digit())
        {
            xml.replace_range(pos..pos + 20, "1970-01-01T00:00:00Z");
            pos += 20;
        } else {
            pos += 1;
        }
    }
}

/// Scrub PII from XML content that we do keep.
fn scrub_xml(xml: &str) -> String {
    let mut xml = xml.to_string();
    remove_element(&mut xml, "<cp:lastModifiedBy>", "</cp:lastModifiedBy>");
    remove_element(&mut xml, "<dc:creator>", "</dc:creator>");
    replace_attr_value(&mut xml, "w:author", "");
    replace_attr_value(&mut xml, "w15:val", "{00000000-0000-0000-0000-000000000000}");
    replace_attr_value(&mut xml, "siteId", "{00000000-0000-0000-0000-000000000000}");
    replace_timestamps(&mut xml);
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
