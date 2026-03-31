use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};

use zip::ZipArchive;

// Configuration defaults matching the Perl script's built-in defaults.
// The config file in the repo overrides some of these, but modern Perl
// (5.26+) no longer loads it because "." was removed from @INC.
const LINE_WIDTH: usize = 80;
const TWIPS_PER_CHAR: f64 = 120.0;
const SHOW_HYPERLINK: bool = false;
const NEWLINE: &str = "\n";

// ---- Low-level string matching helpers ----

/// Find the closing tag `</prefix:tag>` that matches an opening `<prefix:tag`.
/// Returns the byte offset just past `>` of the closing tag, or None.
fn find_close_tag<'a>(s: &'a str, prefix: &str, tag: &str) -> Option<usize> {
    let needle = format!("</{prefix}:{tag}>");
    s.find(&needle).map(|i| i + needle.len())
}

/// Remove all `<prefix:*>...</prefix:*>` blocks (non-nested, greedy on tag name).
fn remove_ns_blocks(s: &mut String, prefix: &str) {
    let open_prefix = format!("<{prefix}:");
    loop {
        let Some(start) = s.find(&open_prefix) else {
            break;
        };
        // Extract the tag name (up to the first space or >).
        let rest = &s[start + open_prefix.len()..];
        let tag_end = rest.find(|c: char| c == ' ' || c == '>').unwrap_or(rest.len());
        let tag = rest[..tag_end].to_string();
        let after_open = &s[start..];
        if let Some(close_end) = find_close_tag(after_open, prefix, &tag) {
            s.replace_range(start..start + close_end, "");
        } else {
            break;
        }
    }
}

/// Remove all `<w:tag ...>...</w:tag>` occurrences for the given tag name.
fn remove_w_element(s: &mut String, tag: &str) {
    let open = format!("<w:{tag}");
    let close = format!("</w:{tag}>");
    loop {
        let Some(start) = s.find(&open) else { break };
        let search_from = &s[start..];
        if let Some(end_rel) = search_from.find(&close) {
            s.replace_range(start..start + end_rel + close.len(), "");
        } else {
            break;
        }
    }
}

/// Extract the value of `attr="..."` starting the search at `start` within `s`.
/// Returns (value, offset_past_closing_quote).
fn extract_attr<'a>(s: &'a str, attr: &str) -> Option<(&'a str, usize)> {
    let needle = format!("{attr}=\"");
    let pos = s.find(&needle)?;
    let val_start = pos + needle.len();
    let val_end = s[val_start..].find('"')? + val_start;
    Some((&s[val_start..val_end], val_end + 1))
}

/// Extract attribute value as a simple accessor.
fn attr_val<'a>(s: &'a str, attr: &str) -> Option<&'a str> {
    extract_attr(s, attr).map(|(v, _)| v)
}

/// Strip all XML tags from a string.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out
}

/// Replace all occurrences of `<open_tag>...</close_tag>` with `replacement`,
/// where open_tag is matched by prefix (may have attributes).
fn replace_element(s: &mut String, open_prefix: &str, close_tag: &str, replacement: &str) {
    loop {
        let Some(start) = s.find(open_prefix) else { break };
        let search = &s[start..];
        if let Some(end_rel) = search.find(close_tag) {
            s.replace_range(start..start + end_rel + close_tag.len(), replacement);
        } else {
            break;
        }
    }
}

/// Parse a decimal integer from a string, returning 0 on failure.
fn parse_usize(s: &str) -> usize {
    s.parse().unwrap_or(0)
}

// ---- Character conversion tables ----

/// Unicode-to-ASCII substitutions matching the Perl %splchars table.
fn special_char(ch: char) -> Option<&'static str> {
    Some(match ch {
        // Latin-1 Supplement
        '\u{00A0}' => " ",
        '\u{00A2}' => "cent",
        '\u{00A3}' => "Pound",
        '\u{00A5}' => "Yen",
        '\u{00A6}' => "|",
        '\u{00A9}' => "(C)",
        '\u{00AB}' => "<<",
        '\u{00AC}' => "-",
        '\u{00AE}' => "(R)",
        '\u{00B1}' => "+-",
        '\u{00B4}' => "'",
        '\u{00B5}' => "u",
        '\u{00BB}' => ">>",
        '\u{00BC}' => "(1/4)",
        '\u{00BD}' => "(1/2)",
        '\u{00BE}' => "(3/4)",
        '\u{00D7}' => "x",
        '\u{00F7}' => "/",
        // Greek
        '\u{03C0}' => "PI",
        // General Punctuation
        '\u{2002}' => "  ",
        '\u{2003}' => "  ",
        '\u{2005}' => " ",
        '\u{2013}' => " - ",
        '\u{2014}' => " -- ",
        '\u{2015}' => "--",
        '\u{2018}' => "`",
        '\u{2019}' => "'",
        '\u{201C}' => "\"",
        '\u{201D}' => "\"",
        '\u{2022}' => "::",
        '\u{2026}' => "...",
        '\u{2030}' => "%.",
        // Currency
        '\u{20AC}' => "Euro",
        // Letterlike Symbols
        '\u{2105}' => "c/o",
        '\u{2117}' => "(P)",
        '\u{2120}' => "(SM)",
        '\u{2122}' => "(TM)",
        '\u{2126}' => "Ohm",
        // Vulgar Fractions
        '\u{2153}' => "(1/3)",
        '\u{2154}' => "(2/3)",
        '\u{2155}' => "(1/5)",
        '\u{2156}' => "(2/5)",
        '\u{2157}' => "(3/5)",
        '\u{2158}' => "(4/5)",
        '\u{2159}' => "(1/6)",
        '\u{215B}' => "(1/8)",
        '\u{215C}' => "(3/8)",
        '\u{215D}' => "(5/8)",
        '\u{215E}' => "(7/8)",
        '\u{215F}' => "1/",
        // Arrows
        '\u{2190}' => "<--",
        '\u{2192}' => "-->",
        '\u{2194}' => "<-->",
        // Mathematical Operators
        '\u{2202}' => "d",
        '\u{221E}' => "infinity",
        '\u{2260}' => "!=",
        '\u{2264}' => "<=",
        '\u{2265}' => ">=",
        // Private Use Area
        '\u{F0B7}' => "*",
        _ => return None,
    })
}

/// Bullet character map matching the Perl %bullets table.
fn bullet_char(ch: char) -> &'static str {
    match ch {
        'o' => "o",
        '\u{F076}' => "::",
        '\u{F0A7}' => "#",
        '\u{F0B7}' => "*",
        '\u{F0D8}' => ">",
        '\u{F0FC}' => "+",
        _ => "oo",
    }
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
}

impl ListState {
    fn new() -> Self {
        Self {
            last_cnt: vec![0],
            twip_stack: vec![0],
            key_stack: vec![None],
        }
    }

    fn format_number(&mut self, level: &LevelDef, key: &str) -> String {
        if matches!(level.format, NumFormat::Bullet) {
            let ch = level.lvl_text.chars().next().unwrap_or('\0');
            return format!("{}{} ", " ".repeat(level.indent_spaces), bullet_char(ch));
        }

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
    let mut map = HashMap::new();
    let mut pos = 0;
    while let Some(start) = xml[pos..].find("<Relationship ") {
        let start = pos + start;
        let Some(end) = xml[start..].find("/>") else { break };
        let tag = &xml[start..start + end + 2];
        pos = start + end + 2;

        let Some(id) = attr_val(tag, "Id") else { continue };
        let Some(typ) = attr_val(tag, "Type") else { continue };
        let Some(target) = attr_val(tag, "Target") else { continue };

        let type_suffix = typ.rsplit('/').next().unwrap_or("");
        map.insert(format!("{type_suffix}:{id}"), target.to_string());
    }
    map
}

fn parse_numbering(
    xml: &str,
) -> (HashMap<String, LevelDef>, Vec<Option<usize>>) {
    let mut abstract_nums: HashMap<String, LevelDef> = HashMap::new();
    let mut num_to_abstract: Vec<Option<usize>> = Vec::new();

    let mut pos = 0;
    while let Some(rel) = xml[pos..].find("<w:abstractNum w:abstractNumId=\"") {
        let start = pos + rel;
        let abstract_id_str = attr_val(&xml[start..], "w:abstractNumId").unwrap_or("0");
        let abstract_id = parse_usize(abstract_id_str);

        let Some(close_rel) = xml[start..].find("</w:abstractNum>") else { break };
        let inner = &xml[start..start + close_rel];
        pos = start + close_rel + "</w:abstractNum>".len();

        // Parse each <w:lvl> within this abstractNum.
        let mut lpos = 0;
        while let Some(lr) = inner[lpos..].find("<w:lvl w:ilvl=\"") {
            let lstart = lpos + lr;
            let ilvl = parse_usize(attr_val(&inner[lstart..], "w:ilvl").unwrap_or("0"));

            // Find the end of this lvl (next <w:lvl or end of inner).
            let lvl_end = inner[lstart + 1..]
                .find("<w:lvl ")
                .map_or(inner.len(), |i| lstart + 1 + i);
            let lvl = &inner[lstart..lvl_end];
            lpos = lvl_end;

            let Some(start_val) = attr_val(lvl, "w:val").and_then(|_|
                // First w:val after <w:start
                lvl.find("<w:start").and_then(|si| attr_val(&lvl[si..], "w:val"))
            ) else { continue };
            let start_num = parse_usize(start_val);

            let Some(num_fmt) = lvl.find("<w:numFmt").and_then(|si| attr_val(&lvl[si..], "w:val")) else { continue };
            let Some(lvl_text) = lvl.find("<w:lvlText").and_then(|si| attr_val(&lvl[si..], "w:val")) else { continue };

            let format = match num_fmt {
                "bullet" => NumFormat::Bullet,
                "decimal" => NumFormat::Decimal,
                "lowerLetter" => NumFormat::LowerLetter,
                "upperLetter" => NumFormat::UpperLetter,
                "lowerRoman" => NumFormat::LowerRoman,
                "upperRoman" => NumFormat::UpperRoman,
                _ => continue,
            };

            let (left, hanging) = if let Some(ind_pos) = lvl.find("<w:ind ") {
                let ind = &lvl[ind_pos..];
                let left = parse_usize(attr_val(ind, "w:left").unwrap_or("0"));
                let hanging = parse_usize(attr_val(ind, "w:hanging").unwrap_or("0"));
                (left, hanging)
            } else {
                (0, 0)
            };

            let indent = ((left as f64 - hanging as f64) / TWIPS_PER_CHAR + 0.5) as usize;
            abstract_nums.insert(
                format!("{abstract_id}:{ilvl}"),
                LevelDef {
                    format,
                    lvl_text: lvl_text.to_string(),
                    start: start_num,
                    indent_spaces: indent,
                    left_twips: left,
                },
            );
        }
    }

    // Parse <w:num w:numId="N"><w:abstractNumId w:val="M"
    let mut pos = 0;
    while let Some(rel) = xml[pos..].find("<w:num w:numId=\"") {
        let start = pos + rel;
        let tag_area = &xml[start..];
        let num_id = parse_usize(attr_val(tag_area, "w:numId").unwrap_or("0"));
        pos = start + 1;

        if let Some(abs_rel) = tag_area.find("<w:abstractNumId") {
            let abs_id = parse_usize(attr_val(&tag_area[abs_rel..], "w:val").unwrap_or("0"));
            if num_id >= num_to_abstract.len() {
                num_to_abstract.resize(num_id + 1, None);
            }
            num_to_abstract[num_id] = Some(abs_id);
        }
    }

    (abstract_nums, num_to_abstract)
}

// ---- Inline processing helpers ----

fn process_hyperlink(
    rid: &str,
    inner_xml: &str,
    rels: &HashMap<String, String>,
) -> String {
    let text = strip_tags(inner_xml);
    if SHOW_HYPERLINK {
        if let Some(url) = rels.get(&format!("hyperlink:{rid}")) {
            if text != *url {
                return format!("{text} [HYPERLINK: {url}]");
            }
        }
    }
    text
}

fn process_paragraph(inner: &str) -> String {
    // Extract justification before stripping tags.
    let align = inner
        .find("<w:jc w:val=\"")
        .and_then(|p| attr_val(&inner[p..], "w:val"))
        .map(|s| s.to_string());

    let text = format!("{}{}", strip_tags(inner), NEWLINE);

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
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match special_char(ch) {
            Some(r) => out.push_str(r),
            None => out.push(ch),
        }
    }
    out
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let rest: String = chars.clone().take(5).collect();
            let replaced = if rest.starts_with("amp;") {
                for _ in 0..4 { chars.next(); }
                "&"
            } else if rest.starts_with("apos;") {
                for _ in 0..5 { chars.next(); }
                "'"
            } else if rest.starts_with("gt;") {
                for _ in 0..3 { chars.next(); }
                ">"
            } else if rest.starts_with("lt;") {
                for _ in 0..3 { chars.next(); }
                "<"
            } else if rest.starts_with("quot;") {
                for _ in 0..5 { chars.next(); }
                "\""
            } else {
                out.push('&');
                continue;
            };
            out.push_str(replaced);
        } else {
            out.push(ch);
        }
    }
    out
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

    // 1. Remove XML declaration.
    if let Some(end) = content.find("?>\n").or(content.find("?>\r\n")) {
        let end = content[end..].find('\n').unwrap() + end + 1;
        if content[..end].starts_with("<?xml ") {
            content.replace_range(..end, "");
        }
    }

    // 2. Remove wp14: and wp: drawing wrappers.
    remove_ns_blocks(&mut content, "wp14");
    remove_ns_blocks(&mut content, "wp");

    // 3. Remove field instructions, field data, and deleted text.
    for tag in &["instrText", "fldData", "delText"] {
        remove_w_element(&mut content, tag);
    }

    // 4. Superscript cross-references -> [N].
    {
        let marker = "<w:vertAlign w:val=\"superscript\"/></w:rPr><w:t>";
        let close = "</w:t>";
        loop {
            let Some(start) = content.find(marker) else { break };
            let after = start + marker.len();
            let Some(end_rel) = content[after..].find(close) else { break };
            let text = content[after..after + end_rel].to_string();
            content.replace_range(start..after + end_rel + close.len(), &format!("[{text}]"));
        }
    }

    // 5. Special empty tags.
    content = content.replace("<w:tab/>", "\t");
    content = content.replace("<w:noBreakHyphen/>", "-");
    content = content.replace("<w:softHyphen/>", " - ");

    // 6. Paragraph borders -> horizontal rule.
    let hr = format!("{}{}", "-".repeat(LINE_WIDTH), NEWLINE);
    replace_element(&mut content, "<w:pBdr>", "</w:pBdr>", &hr);

    // 7. Caps formatting -> uppercase.
    {
        let marker = "<w:caps/>";
        loop {
            let Some(caps_pos) = content.find(marker) else { break };
            // Find the next <w:t> or <w:t ...> after <w:caps/>.
            let after_caps = caps_pos + marker.len();
            let rest = &content[after_caps..];
            let t_pos = rest.find("<w:t>").map(|p| (p, "<w:t>".len()))
                .or_else(|| rest.find("<w:t ").and_then(|p| {
                    rest[p..].find('>').map(|e| (p, e + 1))
                }));
            let Some((t_rel, t_tag_len)) = t_pos else { break };
            let text_start = after_caps + t_rel + t_tag_len;
            let Some(text_end_rel) = content[text_start..].find("</w:t>") else { break };
            let text = content[text_start..text_start + text_end_rel].to_uppercase();
            content.replace_range(caps_pos..text_start + text_end_rel + "</w:t>".len(), &text);
        }
    }

    // 8. Hyperlinks.
    {
        let open = "<w:hyperlink r:id=\"";
        let close = "</w:hyperlink>";
        loop {
            let Some(start) = content.find(open) else { break };
            let rid_start = start + open.len();
            let Some(rid_end_rel) = content[rid_start..].find('"') else { break };
            let rid = content[rid_start..rid_start + rid_end_rel].to_string();
            let Some(gt_rel) = content[start..].find('>') else { break };
            let inner_start = start + gt_rel + 1;
            let Some(close_rel) = content[inner_start..].find(close) else { break };
            let inner = content[inner_start..inner_start + close_rel].to_string();
            let replacement = process_hyperlink(&rid, &inner, &hyperlinks);
            content.replace_range(start..inner_start + close_rel + close.len(), &replacement);
        }
    }

    // 9. List numbering.
    {
        let marker = "<w:numPr><w:ilvl w:val=\"";
        let mut buf = String::with_capacity(content.len());
        let mut pos = 0;
        while let Some(rel) = content[pos..].find(marker) {
            let start = pos + rel;
            buf.push_str(&content[pos..start]);

            let after_ilvl = start + marker.len();
            let ilvl_end = content[after_ilvl..].find('"').unwrap_or(0) + after_ilvl;
            let ilvl = parse_usize(&content[after_ilvl..ilvl_end]);

            let num_marker = "<w:numId w:val=\"";
            if let Some(num_rel) = content[ilvl_end..].find(num_marker) {
                let num_val_start = ilvl_end + num_rel + num_marker.len();
                let num_val_end = content[num_val_start..].find('"').unwrap_or(0) + num_val_start;
                let num_id = parse_usize(&content[num_val_start..num_val_end]);

                // Find end of the match (past the closing />).
                let match_end = content[num_val_end..].find("/>").map_or(num_val_end, |r| num_val_end + r + 2);

                if let Some(&Some(abs_id)) = num_to_abstract.get(num_id) {
                    let key = format!("{abs_id}:{ilvl}");
                    if let Some(level) = abstract_nums.get(&key).cloned() {
                        buf.push_str(&list_state.format_number(&level, &key));
                    }
                }
                pos = match_end;
            } else {
                buf.push_str(&content[start..start + 1]);
                pos = start + 1;
            }
        }
        buf.push_str(&content[pos..]);
        content = buf;
    }

    // 10. Indentation.
    {
        let marker = "<w:ind w:";
        let mut buf = String::with_capacity(content.len());
        let mut pos = 0;
        while let Some(rel) = content[pos..].find(marker) {
            let start = pos + rel;
            buf.push_str(&content[pos..start]);

            let tag = &content[start..];
            let tag_end = tag.find('>').unwrap_or(tag.len()) + 1;
            let tag_str = &content[start..start + tag_end];

            let val = attr_val(tag_str, "w:left")
                .or_else(|| attr_val(tag_str, "w:firstLine"))
                .map(|v| v.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(0.0);
            let hanging = attr_val(tag_str, "w:hanging")
                .map(|v| v.parse::<f64>().unwrap_or(0.0))
                .unwrap_or(0.0);
            let spaces = ((val - hanging) / TWIPS_PER_CHAR + 0.5) as usize;
            for _ in 0..spaces {
                buf.push(' ');
            }
            pos = start + tag_end;
        }
        buf.push_str(&content[pos..]);
        content = buf;
    }

    // 11. Self-closing empty paragraphs and line breaks -> newline.
    content = content.replace("<w:br/>", NEWLINE);
    {
        // Match <w:p .../>  (self-closing, must have at least one attribute).
        let marker = "<w:p ";
        let mut buf = String::with_capacity(content.len());
        let mut pos = 0;
        while let Some(rel) = content[pos..].find(marker) {
            let start = pos + rel;
            let tag = &content[start..];
            if let Some(gt) = tag.find('>') {
                if tag.as_bytes()[gt - 1] == b'/' {
                    buf.push_str(&content[pos..start]);
                    buf.push_str(NEWLINE);
                    pos = start + gt + 1;
                    continue;
                }
            }
            buf.push_str(&content[pos..start + marker.len()]);
            pos = start + marker.len();
        }
        buf.push_str(&content[pos..]);
        content = buf;
    }

    // 12. Process paragraph content.
    {
        let open = "<w:p ";  // Opening <w:p with attributes
        let close = "</w:p>";
        let mut buf = String::with_capacity(content.len());
        let mut pos = 0;
        while let Some(rel) = content[pos..].find(open) {
            let start = pos + rel;
            buf.push_str(&content[pos..start]);

            let tag = &content[start..];
            let Some(gt) = tag.find('>') else {
                buf.push_str(&content[start..]);
                pos = content.len();
                break;
            };
            // Skip self-closing (already handled).
            if tag.as_bytes()[gt - 1] == b'/' {
                buf.push_str(&content[start..start + gt + 1]);
                pos = start + gt + 1;
                continue;
            }
            let inner_start = start + gt + 1;
            if let Some(close_rel) = content[inner_start..].find(close) {
                let inner = &content[inner_start..inner_start + close_rel];
                buf.push_str(&process_paragraph(inner));
                pos = inner_start + close_rel + close.len();
            } else {
                buf.push_str(&content[start..start + gt + 1]);
                pos = start + gt + 1;
            }
        }
        buf.push_str(&content[pos..]);
        content = buf;
    }

    // 13. Strip any remaining XML tags.
    content = strip_tags(&content);

    // 14. Convert non-ASCII special characters to ASCII.
    content = convert_special_chars(&content);

    // 15. Decode XML character entities.
    content = decode_entities(&content);

    Ok(content)
}
