/// In-memory generation of minimal .docx test fixtures.
///
/// Each public function returns a complete .docx ZIP archive as bytes.

use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

fn build_docx(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zw = ZipWriter::new(&mut buf);
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for &(name, content) in parts {
            zw.start_file(name, opts).unwrap();
            zw.write_all(content.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }
    buf.into_inner()
}

// ---- OOXML boilerplate ----

fn content_types(extras: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{extras}</Types>"#
    )
}

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

const NS: &str = concat!(
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" "#,
    r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#,
);

fn wrap_document(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document {NS}><w:body>{body}</w:body></w:document>"
    )
}

// ---- Paragraph helpers ----

const A: &str = r#"w:rsidR="00000000" w:rsidRDefault="00000000""#;

fn p(inner: &str) -> String {
    format!("<w:p {A}>{inner}</w:p>")
}
fn sp() -> String {
    format!("<w:p {A}/>")
}
fn t(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

// ---- basic ----

pub fn basic_docx() -> Vec<u8> {
    let doc_rels = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>"#,
        r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://git-scm.com" TargetMode="External"/>"#,
        r#"</Relationships>"#,
    );

    let body = [
        p(&t("Plain paragraph.")),
        p(&format!("{}{}", t("Two "), t("runs merged."))),
        p(&format!("{}<w:r><w:br/></w:r>{}", t("Line one."), t("Line two."))),
        p(&format!("{}<w:r><w:tab/></w:r>{}", t("Col1"), t("Col2"))),
        p(&format!("{}<w:r><w:noBreakHyphen/></w:r>{}", t("non"), t("breaking"))),
        p(&format!("{}<w:r><w:softHyphen/></w:r>{}", t("soft"), t("hyphen"))),
        p(r#"<w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/></w:pBdr></w:pPr>"#),
        p(r#"<w:r><w:rPr><w:caps/></w:rPr><w:t>shouting text</w:t></w:r>"#),
        p(&format!(
            "{}{}{}",
            t("See note"),
            r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:t>1</w:t></w:r>"#,
            t(" for details.")
        )),
        p(&format!(
            "{}{}{}",
            t("Visit "),
            format!(r#"<w:hyperlink r:id="rId1" w:history="1">{}</w:hyperlink>"#, t("our site")),
            t(".")
        )),
        p(&format!(
            r#"<w:hyperlink r:id="rId2" w:history="1">{}</w:hyperlink>"#,
            t("https://git-scm.com")
        )),
        p(&format!(r#"<w:pPr><w:ind w:left="720"/></w:pPr>{}"#, t("Indented text."))),
        p(&format!(r#"<w:pPr><w:ind w:firstLine="480"/></w:pPr>{}"#, t("First-line indent."))),
        p(&format!(r#"<w:pPr><w:ind w:left="960" w:hanging="240"/></w:pPr>{}"#, t("Hanging indent."))),
        p(&t("\u{00A9} \u{00AE} \u{2122} \u{00BD} \u{20AC}")),
        p(&t("\u{201C}Hello\u{201D} \u{2014} World")),
        p(&t("AT&amp;T, 5 &gt; 3, &apos;quoted&apos;")),
        p(&format!(
            "{}{}{}",
            t("visible"),
            r#"<w:del w:id="1" w:author="test"><w:r><w:delText>DELETED</w:delText></w:r></w:del>"#,
            t(" text")
        )),
        p(&format!(
            "{}{}{}",
            t("before"),
            r#"<w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>"#,
            t("after")
        )),
        p(&format!(
            "{}{}{}",
            t("text"),
            r#"<wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="914400" cy="914400"/></wp:inline>"#,
            t("more")
        )),
        p(&format!(
            "{}{}{}",
            t("before wp14"),
            r#"<wp14:sizeRelH relativeFrom="page"><wp14:pctWidth>0</wp14:pctWidth></wp14:sizeRelH>"#,
            t("after wp14")
        )),
        sp(),
    ]
    .join("");

    let ct = content_types("");
    let doc = wrap_document(&body);
    build_docx(&[
        ("[Content_Types].xml", &ct),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", doc_rels),
        ("word/document.xml", &doc),
    ])
}

// ---- lists ----

const BULLET: char = '\u{F0B7}';
const SQUARE_BUL: char = '\u{F0A7}';

struct LevelSpec {
    ilvl: usize,
    start: usize,
    num_fmt: &'static str,
    lvl_text: String,
    left: usize,
    hanging: usize,
    font: Option<&'static str>,
}

fn abstract_num(id: usize, nsid: &mut u32, tmpl: &mut u32, levels: &[LevelSpec]) -> String {
    let nsid_str = format!("{:08X}", *nsid);
    *nsid += 1;
    let tmpl_str = format!("{:08X}", *tmpl);
    *tmpl += 1;

    let mut lvl_xml = String::new();
    for i in 0..9usize {
        if let Some(l) = levels.iter().find(|l| l.ilvl == i) {
            let rpr = match l.font {
                Some(f) => format!(
                    r#"<w:rPr><w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:hint="default"/></w:rPr>"#
                ),
                None => String::new(),
            };
            lvl_xml += &format!(
                r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="{start}"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{txt}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="{hang}"/></w:pPr>{rpr}</w:lvl>"#,
                ilvl = i, start = l.start, fmt = l.num_fmt,
                txt = l.lvl_text, left = l.left, hang = l.hanging,
            );
        } else {
            let left = 720 * (i + 1);
            lvl_xml += &format!(
                r#"<w:lvl w:ilvl="{ilvl}" w:tentative="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%{n}."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr></w:lvl>"#,
                ilvl = i, n = i + 1, left = left,
            );
        }
    }

    format!(
        r#"<w:abstractNum w:abstractNumId="{id}"><w:nsid w:val="{nsid_str}"/><w:multiLevelType w:val="multilevel"/><w:tmpl w:val="{tmpl_str}"/>{lvl_xml}</w:abstractNum>"#,
    )
}

fn num_instance(num_id: usize, abstract_num_id: usize) -> String {
    format!(r#"<w:num w:numId="{num_id}"><w:abstractNumId w:val="{abstract_num_id}"/></w:num>"#)
}

fn list_p(num_id: usize, ilvl: usize, text: &str) -> String {
    format!(
        r#"<w:p {A}><w:pPr><w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num_id}"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#,
    )
}

pub fn lists_docx() -> Vec<u8> {
    let mut nsid = 0x10000000u32;
    let mut tmpl = 0xA0000000u32;

    let numbering_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">{abs0}{abs1}{abs2}{abs3}{abs4}{n1}{n2}{n3}{n4}{n5}</w:numbering>"#,
        abs0 = abstract_num(0, &mut nsid, &mut tmpl, &[
            LevelSpec { ilvl: 0, start: 1, num_fmt: "bullet", lvl_text: BULLET.to_string(), left: 720, hanging: 360, font: Some("Symbol") },
        ]),
        abs1 = abstract_num(1, &mut nsid, &mut tmpl, &[
            LevelSpec { ilvl: 0, start: 1, num_fmt: "decimal", lvl_text: "%1.".into(), left: 720, hanging: 360, font: None },
        ]),
        abs2 = abstract_num(2, &mut nsid, &mut tmpl, &[
            LevelSpec { ilvl: 0, start: 1, num_fmt: "lowerLetter", lvl_text: "%1)".into(), left: 720, hanging: 360, font: None },
        ]),
        abs3 = abstract_num(3, &mut nsid, &mut tmpl, &[
            LevelSpec { ilvl: 0, start: 1, num_fmt: "upperRoman", lvl_text: "%1.".into(), left: 720, hanging: 360, font: None },
        ]),
        abs4 = abstract_num(4, &mut nsid, &mut tmpl, &[
            LevelSpec { ilvl: 0, start: 1, num_fmt: "decimal", lvl_text: "%1.".into(), left: 720, hanging: 360, font: None },
            LevelSpec { ilvl: 1, start: 1, num_fmt: "bullet", lvl_text: SQUARE_BUL.to_string(), left: 1440, hanging: 360, font: Some("Wingdings") },
        ]),
        n1 = num_instance(1, 0), n2 = num_instance(2, 1),
        n3 = num_instance(3, 2), n4 = num_instance(4, 3),
        n5 = num_instance(5, 4),
    );

    let body = [
        list_p(1, 0, "Bullet one"), list_p(1, 0, "Bullet two"), list_p(1, 0, "Bullet three"),
        sp(),
        list_p(2, 0, "First item"), list_p(2, 0, "Second item"), list_p(2, 0, "Third item"),
        sp(),
        list_p(3, 0, "Alpha"), list_p(3, 0, "Bravo"), list_p(3, 0, "Charlie"),
        sp(),
        list_p(4, 0, "First"), list_p(4, 0, "Second"), list_p(4, 0, "Third"),
        sp(),
        list_p(5, 0, "Parent one"), list_p(5, 1, "Child A"), list_p(5, 1, "Child B"),
        list_p(5, 0, "Parent two"), list_p(5, 1, "Child C"),
    ]
    .join("");

    let ct = content_types(
        r#"<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#,
    );
    let lists_doc_rels = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
        r#"</Relationships>"#,
    );
    let doc = wrap_document(&body);
    build_docx(&[
        ("[Content_Types].xml", &ct),
        ("_rels/.rels", ROOT_RELS),
        ("word/_rels/document.xml.rels", lists_doc_rels),
        ("word/document.xml", &doc),
        ("word/numbering.xml", &numbering_xml),
    ])
}
