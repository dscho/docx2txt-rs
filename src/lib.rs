use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};

use regex::{Captures, Regex};
use zip::ZipArchive;

// Configuration defaults matching the Perl script's built-in defaults.
// The config file in the repo overrides some of these, but modern Perl
// (5.26+) no longer loads it because "." was removed from @INC.
const LINE_WIDTH: usize = 80;
const TWIPS_PER_CHAR: f64 = 120.0;
const SHOW_HYPERLINK: bool = false;
const NEWLINE: &str = "\n";

// ---- Character conversion tables ----

/// Unicode-to-ASCII substitutions matching the Perl %splchars table.
fn special_chars_map() -> HashMap<char, &'static str> {
    HashMap::from([
        // Latin-1 Supplement (Perl \xC2 and \xC3 prefixes)
        ('\u{00A0}', " "),
        ('\u{00A2}', "cent"),
        ('\u{00A3}', "Pound"),
        ('\u{00A5}', "Yen"),
        ('\u{00A6}', "|"),
        ('\u{00A9}', "(C)"),
        ('\u{00AB}', "<<"),
        ('\u{00AC}', "-"),
        ('\u{00AE}', "(R)"),
        ('\u{00B1}', "+-"),
        ('\u{00B4}', "'"),
        ('\u{00B5}', "u"),
        ('\u{00BB}', ">>"),
        ('\u{00BC}', "(1/4)"),
        ('\u{00BD}', "(1/2)"),
        ('\u{00BE}', "(3/4)"),
        ('\u{00D7}', "x"),
        ('\u{00F7}', "/"),
        // Greek (Perl \xCF prefix)
        ('\u{03C0}', "PI"),
        // General Punctuation (Perl \xE2\x80 prefix)
        ('\u{2002}', "  "),
        ('\u{2003}', "  "),
        ('\u{2005}', " "),
        ('\u{2013}', " - "),
        ('\u{2014}', " -- "),
        ('\u{2015}', "--"),
        ('\u{2018}', "`"),
        ('\u{2019}', "'"),
        ('\u{201C}', "\""),
        ('\u{201D}', "\""),
        ('\u{2022}', "::"),
        ('\u{2026}', "..."),
        ('\u{2030}', "%."),
        // Currency (Perl \xE2\x82 prefix)
        ('\u{20AC}', "Euro"),
        // Letterlike Symbols (Perl \xE2\x84 prefix)
        ('\u{2105}', "c/o"),
        ('\u{2117}', "(P)"),
        ('\u{2120}', "(SM)"),
        ('\u{2122}', "(TM)"),
        ('\u{2126}', "Ohm"),
        // Vulgar Fractions (Perl \xE2\x85 prefix)
        ('\u{2153}', "(1/3)"),
        ('\u{2154}', "(2/3)"),
        ('\u{2155}', "(1/5)"),
        ('\u{2156}', "(2/5)"),
        ('\u{2157}', "(3/5)"),
        ('\u{2158}', "(4/5)"),
        ('\u{2159}', "(1/6)"),
        ('\u{215B}', "(1/8)"),
        ('\u{215C}', "(3/8)"),
        ('\u{215D}', "(5/8)"),
        ('\u{215E}', "(7/8)"),
        ('\u{215F}', "1/"),
        // Arrows (Perl \xE2\x86 prefix)
        ('\u{2190}', "<--"),
        ('\u{2192}', "-->"),
        ('\u{2194}', "<-->"),
        // Mathematical Operators (Perl \xE2\x88 and \xE2\x89 prefixes)
        ('\u{2202}', "d"),
        ('\u{221E}', "infinity"),
        ('\u{2260}', "!="),
        ('\u{2264}', "<="),
        ('\u{2265}', ">="),
        // Private Use Area (Perl \xEF\x82 prefix)
        ('\u{F0B7}', "*"),
    ])
}

/// Bullet character map matching the Perl %bullets table.
fn bullet_char_map() -> HashMap<char, &'static str> {
    HashMap::from([
        ('o', "o"),
        ('\u{F076}', "::"),
        ('\u{F0A7}', "#"),
        ('\u{F0B7}', "*"),
        ('\u{F0D8}', ">"),
        ('\u{F0FC}', "+"),
    ])
}

// ---- Numbering format functions ----

fn lower_roman(n: usize) -> String {
    const CODES: &[(&str, usize)] = &[
        ("m", 1000), ("cm", 900), ("d", 500), ("cd", 400),
        ("c", 100),  ("xc", 90),  ("l", 50),  ("xl", 40),
        ("x", 10),   ("ix", 9),   ("v", 5),   ("iv", 4),
        ("i", 1),
    ];
    let mut result = String::new();
    let mut remaining = n;
    for &(code, val) in CODES {
        while remaining >= val {
            result.push_str(code);
            remaining -= val;
        }
    }
    result
}

fn upper_roman(n: usize) -> String {
    lower_roman(n).to_uppercase()
}

fn lower_letter(n: usize) -> String {
    let idx = (n - 1) % 26;
    let repeat = (n - 1) / 26 + 1;
    let ch = (b'a' + idx as u8) as char;
    std::iter::repeat(ch).take(repeat).collect()
}

fn upper_letter(n: usize) -> String {
    lower_letter(n).to_uppercase()
}

// ---- List numbering state machine ----

#[derive(Clone)]
enum NumFormat {
    Bullet,
    Decimal,
    LowerLetter,
    UpperLetter,
    LowerRoman,
    UpperRoman,
}

#[derive(Clone)]
struct LevelDef {
    format: NumFormat,
    lvl_text: String,
    start: usize,
    indent_spaces: usize,
    left_twips: usize,
}

struct ListState {
    last_cnt: Vec<usize>,
    twip_stack: Vec<usize>,
    key_stack: Vec<Option<String>>,
    bullet_map: HashMap<char, &'static str>,
}

impl ListState {
    fn new() -> Self {
        Self {
            last_cnt: vec![0],
            twip_stack: vec![0],
            key_stack: vec![None],
            bullet_map: bullet_char_map(),
        }
    }

    fn format_number(&mut self, level: &LevelDef, key: &str) -> String {
        if matches!(level.format, NumFormat::Bullet) {
            let ch = level.lvl_text.chars().next().unwrap_or('\0');
            let bullet_str = self.bullet_map.get(&ch).copied().unwrap_or("oo");
            return format!("{}{} ", " ".repeat(level.indent_spaces), bullet_str);
        }

        // Non-bullet: maintain the nesting stack.
        if level.left_twips < *self.twip_stack.last().unwrap() {
            while self.twip_stack.len() > 1
                && *self.twip_stack.last().unwrap() > level.left_twips
            {
                self.twip_stack.pop();
                self.key_stack.pop();
                self.last_cnt.pop();
            }
        }

        let ssiz = self.last_cnt.len();
        if level.left_twips == self.twip_stack[ssiz - 1] {
            if self.key_stack[ssiz - 1].as_deref() == Some(key) {
                *self.last_cnt.last_mut().unwrap() += 1;
            } else {
                self.key_stack[ssiz - 1] = Some(key.to_string());
                self.last_cnt[ssiz - 1] = level.start;
            }
        } else {
            self.twip_stack.push(level.left_twips);
            self.key_stack.push(Some(key.to_string()));
            self.last_cnt.push(level.start);
        }

        let ssiz = self.last_cnt.len();
        let ccnt = self.last_cnt[ssiz - 1];

        let formatted = match level.format {
            NumFormat::Decimal => ccnt.to_string(),
            NumFormat::LowerLetter => lower_letter(ccnt),
            NumFormat::UpperLetter => upper_letter(ccnt),
            NumFormat::LowerRoman => lower_roman(ccnt),
            NumFormat::UpperRoman => upper_roman(ccnt),
            NumFormat::Bullet => unreachable!(),
        };

        // Replace the last %N placeholder with the formatted counter,
        // then replace remaining %N placeholders with parent counters.
        let mut text = level.lvl_text.clone();
        if let Some(pos) = text.rfind('%') {
            let tail = text[pos + 2..].to_string();
            text = format!("{}{}{}", &text[..pos], formatted, tail);
        }
        let mut i = ssiz as isize - 2;
        while let Some(pos) = text.rfind('%') {
            if i >= 0 {
                let tail = text[pos + 2..].to_string();
                text = format!("{}{}{}", &text[..pos], self.last_cnt[i as usize], tail);
                i -= 1;
            } else {
                break;
            }
        }

        format!("{}{} ", " ".repeat(level.indent_spaces), text)
    }
}

// ---- OOXML parsing ----

fn extract_string<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut s = String::new();
    file.read_to_string(&mut s).ok()?;
    Some(s)
}

fn parse_rels(xml: &str) -> HashMap<String, String> {
    let re = Regex::new(
        r#"<Relationship Id="([^"]*)" Type="[^"]*?/([^/"]*)" Target="([^"]*)"[^/]*/>"#,
    )
    .unwrap();
    let mut map = HashMap::new();
    for caps in re.captures_iter(xml) {
        map.insert(format!("{}:{}", &caps[2], &caps[1]), caps[3].to_string());
    }
    map
}

fn parse_numbering(
    xml: &str,
) -> (HashMap<String, LevelDef>, Vec<Option<usize>>) {
    let mut abstract_nums: HashMap<String, LevelDef> = HashMap::new();
    let mut num_to_abstract: Vec<Option<usize>> = Vec::new();

    let abstract_re = Regex::new(
        r#"<w:abstractNum w:abstractNumId="(\d+)">(.*?)</w:abstractNum>"#,
    )
    .unwrap();
    let level_re = Regex::new(concat!(
        r#"<w:lvl w:ilvl="(\d+)"[^>]*>"#,
        r#"<w:start w:val="(\d+)"[^>]*>"#,
        r#"<w:numFmt w:val="([^"]*)"[^>]*>"#,
        r#".*?<w:lvlText w:val="([^"]*)"[^>]*>"#,
        r#".*?<w:ind w:left="(\d+)" w:hanging="(\d+)"[^>]*>"#,
    ))
    .unwrap();

    for abs_caps in abstract_re.captures_iter(xml) {
        let abstract_id: usize = abs_caps[1].parse().unwrap_or(0);
        let inner = &abs_caps[2];

        for lc in level_re.captures_iter(inner) {
            let ilvl: usize = lc[1].parse().unwrap_or(0);
            let start: usize = lc[2].parse().unwrap_or(1);
            let left: usize = lc[5].parse().unwrap_or(0);
            let hanging: usize = lc[6].parse().unwrap_or(0);

            let format = match &lc[3] {
                "bullet" => NumFormat::Bullet,
                "decimal" => NumFormat::Decimal,
                "lowerLetter" => NumFormat::LowerLetter,
                "upperLetter" => NumFormat::UpperLetter,
                "lowerRoman" => NumFormat::LowerRoman,
                "upperRoman" => NumFormat::UpperRoman,
                _ => continue,
            };

            let indent =
                ((left as f64 - hanging as f64) / TWIPS_PER_CHAR + 0.5) as usize;
            abstract_nums.insert(
                format!("{}:{}", abstract_id, ilvl),
                LevelDef {
                    format,
                    lvl_text: lc[4].to_string(),
                    start,
                    indent_spaces: indent,
                    left_twips: left,
                },
            );
        }
    }

    let num_re = Regex::new(
        r#"<w:num w:numId="(\d+)"><w:abstractNumId w:val="(\d+)""#,
    )
    .unwrap();
    for caps in num_re.captures_iter(xml) {
        let num_id: usize = caps[1].parse().unwrap_or(0);
        let abstract_id: usize = caps[2].parse().unwrap_or(0);
        if num_id >= num_to_abstract.len() {
            num_to_abstract.resize(num_id + 1, None);
        }
        num_to_abstract[num_id] = Some(abstract_id);
    }

    (abstract_nums, num_to_abstract)
}

// ---- Inline processing helpers ----

fn process_hyperlink(
    rid: &str,
    inner_xml: &str,
    rels: &HashMap<String, String>,
) -> String {
    let tag_re = Regex::new(r"<[^>]*?>").unwrap();
    let text = tag_re.replace_all(inner_xml, "").to_string();

    if SHOW_HYPERLINK {
        if let Some(url) = rels.get(&format!("hyperlink:{}", rid)) {
            if text != *url {
                return format!("{} [HYPERLINK: {}]", text, url);
            }
        }
    }
    text
}

fn process_paragraph(inner: &str) -> String {
    let jc_re = Regex::new(r#"<w:jc w:val="([^"]*)"/>"#).unwrap();
    let align = jc_re.captures(inner).map(|c| c[1].to_string());

    let tag_re = Regex::new(r"<.*?>").unwrap();
    let text = format!("{}{}", tag_re.replace_all(inner, ""), NEWLINE);

    match align.as_deref() {
        Some(a @ ("center" | "right")) => justify(a, &text),
        _ => text,
    }
}

fn justify(align: &str, text: &str) -> String {
    let len = text.len();
    match align {
        "center" if len < LINE_WIDTH - 1 => {
            format!("{}{}", " ".repeat((LINE_WIDTH - len) / 2), text)
        }
        "right" if len < LINE_WIDTH => {
            format!("{}{}", " ".repeat(LINE_WIDTH - len), text)
        }
        _ => text.to_string(),
    }
}

fn convert_special_chars(input: &str) -> String {
    let map = special_chars_map();
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match map.get(&ch) {
            Some(r) => out.push_str(r),
            None => out.push(ch),
        }
    }
    out
}

fn decode_entities(input: &str) -> String {
    let re = Regex::new(r"(?i)&(amp|apos|gt|lt|quot);").unwrap();
    re.replace_all(input, |caps: &Captures| {
        match caps[1].to_ascii_lowercase().as_str() {
            "amp" => "&",
            "apos" => "'",
            "gt" => ">",
            "lt" => "<",
            "quot" => "\"",
            _ => "",
        }
    })
    .to_string()
}

// ---- Main conversion pipeline ----

pub fn convert(input: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    let mut archive = ZipArchive::new(Cursor::new(input))?;

    let rels_xml =
        extract_string(&mut archive, "word/_rels/document.xml.rels")
            .unwrap_or_default();
    let numbering_xml =
        extract_string(&mut archive, "word/numbering.xml")
            .unwrap_or_default();
    let mut content =
        extract_string(&mut archive, "word/document.xml")
            .ok_or("word/document.xml not found")?;

    let hyperlinks = parse_rels(&rels_xml);
    let (abstract_nums, num_to_abstract) = parse_numbering(&numbering_xml);
    let mut list_state = ListState::new();

    // 1. Remove XML declaration (first occurrence only).
    let re = Regex::new(r"<\?xml [^?]*\?>\r?\n").unwrap();
    content = re.replace(&content, "").into_owned();

    // 2. Remove wp14: and wp: drawing wrappers.
    let re = Regex::new(r"<wp14:[^>]*>.*?</wp14:[^>]*>").unwrap();
    content = re.replace_all(&content, "").into_owned();
    let re = Regex::new(r"<wp:[^>]*>.*?</wp:[^>]*>").unwrap();
    content = re.replace_all(&content, "").into_owned();

    // 3. Remove field instructions, field data, and deleted text.
    for tag in &["instrText", "fldData", "delText"] {
        let re = Regex::new(&format!(r"(?s)<w:{t}[^>]*>.*?</w:{t}>", t = tag))
            .unwrap();
        content = re.replace_all(&content, "").into_owned();
    }

    // 4. Superscript cross-references -> [N].
    let re = Regex::new(
        r#"<w:vertAlign w:val="superscript"/></w:rPr><w:t>(.*?)</w:t>"#,
    )
    .unwrap();
    content = re.replace_all(&content, "[$1]").into_owned();

    // 5. Special empty tags.
    content = content.replace("<w:tab/>", "\t");
    content = content.replace("<w:noBreakHyphen/>", "-");
    content = content.replace("<w:softHyphen/>", " - ");

    // 6. Paragraph borders -> horizontal rule.
    let hr = format!("{}{}", "-".repeat(LINE_WIDTH), NEWLINE);
    let re = Regex::new(r"<w:pBdr>.*?</w:pBdr>").unwrap();
    content = re.replace_all(&content, hr.as_str()).into_owned();

    // 7. Caps formatting -> uppercase.
    let re =
        Regex::new(r"<w:caps/>.*?(?:<w:t>|<w:t [^>]+>)(.*?)</w:t>").unwrap();
    content = re
        .replace_all(&content, |caps: &Captures| caps[1].to_uppercase())
        .into_owned();

    // 8. Hyperlinks.
    let re = Regex::new(
        r#"<w:hyperlink r:id="([^"]*)"[^>]*>(.*?)</w:hyperlink>"#,
    )
    .unwrap();
    content = re
        .replace_all(&content, |caps: &Captures| {
            process_hyperlink(&caps[1], &caps[2], &hyperlinks)
        })
        .into_owned();

    // 9. List numbering (needs mutable state, so iterate manually).
    let re = Regex::new(
        r#"<w:numPr><w:ilvl w:val="(\d+)"/><w:numId w:val="(\d+)"/>"#,
    )
    .unwrap();
    let mut buf = String::with_capacity(content.len());
    let mut last_end = 0;
    for caps in re.captures_iter(&content) {
        let m = caps.get(0).unwrap();
        buf.push_str(&content[last_end..m.start()]);

        let ilvl: usize = caps[1].parse().unwrap_or(0);
        let num_id: usize = caps[2].parse().unwrap_or(0);
        if let Some(&Some(abs_id)) = num_to_abstract.get(num_id) {
            let key = format!("{}:{}", abs_id, ilvl);
            if let Some(level) = abstract_nums.get(&key).cloned() {
                buf.push_str(&list_state.format_number(&level, &key));
            }
        }
        last_end = m.end();
    }
    buf.push_str(&content[last_end..]);
    content = buf;

    // 10. Indentation.
    let re = Regex::new(
        r#"<w:ind w:(?:left|firstLine)="(\d+)"(?: w:hanging="(\d+)")?[^>]*>"#,
    )
    .unwrap();
    content = re
        .replace_all(&content, |caps: &Captures| {
            let val: f64 = caps[1].parse().unwrap_or(0.0);
            let hanging: f64 = caps
                .get(2)
                .map_or(0.0, |m| m.as_str().parse().unwrap_or(0.0));
            " ".repeat(((val - hanging) / TWIPS_PER_CHAR + 0.5) as usize)
        })
        .into_owned();

    // 11. Self-closing empty paragraphs and line breaks -> newline.
    let re = Regex::new(r"<w:p [^/>]+?/>|<w:br/>").unwrap();
    content = re.replace_all(&content, NEWLINE).into_owned();

    // 12. Process paragraph content (may span lines).
    let re = Regex::new(r"(?s)<w:p[^>]+?>(.*?)</w:p>").unwrap();
    content = re
        .replace_all(&content, |caps: &Captures| {
            process_paragraph(&caps[1])
        })
        .into_owned();

    // 13. Strip any remaining XML tags.
    let re = Regex::new(r"<.*?>").unwrap();
    content = re.replace_all(&content, "").into_owned();

    // 14. Convert non-ASCII special characters to ASCII.
    content = convert_special_chars(&content);

    // 15. Decode XML character entities.
    content = decode_entities(&content);

    Ok(content)
}
