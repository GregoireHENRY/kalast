//! The script panel as a code editor: Python and Rust in colour, and the
//! indentation keys VS Code has.
//!
//! Coloured by the rules of Catppuccin's VS Code theme -- keywords mauve,
//! strings green, numbers and constants peach, functions blue, types yellow,
//! comments dim and italic, operators sky -- or VS Code's Dark+ with the
//! dark theme. By a tokenizer of its own rather than a grammar engine: two
//! languages read for colour and nothing else, where `syntect` would embed a
//! megabyte of TextMate grammars.

use crate::app::config::UiTheme;
use egui::text::{CCursor, CCursorRange, LayoutJob, LayoutSection, TextFormat};
use egui::Color32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Python,
    Rust,
    Plain,
}

impl Lang {
    /// By the file's extension; a script with no name yet is Python, which
    /// is what a new one in kalast is.
    pub fn of(path: &str) -> Self {
        let path = path.trim_end().to_lowercase();
        if path.ends_with(".rs") {
            Lang::Rust
        } else if path.is_empty() || path.ends_with(".py") || path.ends_with(".pyi") {
            Lang::Python
        } else {
            Lang::Plain
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Text,
    Keyword,
    String,
    Number,
    Constant,
    Comment,
    Function,
    Type,
    SelfWord,
    Decorator,
    Operator,
    Punctuation,
    Lifetime,
}

/// What each kind is drawn in, and whether in italics.
pub struct Palette {
    colors: [(Color32, bool); 13],
    /// Line numbers, and the current one's.
    pub gutter: Color32,
    pub gutter_current: Color32,
    /// The band behind the line the cursor is on.
    pub current_line: Color32,
    /// The python tab's `>>>`, and an error's lines: Python's own REPL draws
    /// its prompt in magenta and its tracebacks in red.
    pub prompt: Color32,
    pub error: Color32,
}

impl Palette {
    fn of(&self, kind: Kind) -> (Color32, bool) {
        self.colors[kind as usize]
    }
}

pub fn palette(theme: UiTheme) -> Palette {
    let c = Color32::from_rgb;
    match theme {
        UiTheme::CatppuccinMocha => {
            let (text, overlay2) = (c(0xcd, 0xd6, 0xf4), c(0x93, 0x99, 0xb2));
            Palette {
                colors: [
                    (text, false),                     // Text
                    (c(0xcb, 0xa6, 0xf7), false),      // Keyword: mauve
                    (c(0xa6, 0xe3, 0xa1), false),      // String: green
                    (c(0xfa, 0xb3, 0x87), false),      // Number: peach
                    (c(0xfa, 0xb3, 0x87), false),      // Constant: peach
                    (overlay2, true),                  // Comment: overlay 2
                    (c(0x89, 0xb4, 0xfa), false),      // Function: blue
                    (c(0xf9, 0xe2, 0xaf), false),      // Type: yellow
                    (c(0xf3, 0x8b, 0xa8), true),       // self: red
                    (c(0xfa, 0xb3, 0x87), false),      // Decorator: peach
                    (c(0x89, 0xdc, 0xeb), false),      // Operator: sky
                    (overlay2, false),                 // Punctuation
                    (c(0xeb, 0xa0, 0xac), true),       // Lifetime: maroon
                ],
                gutter: c(0x6c, 0x70, 0x86),
                gutter_current: c(0xb4, 0xbe, 0xfe),
                current_line: Color32::from_rgba_unmultiplied(0x31, 0x32, 0x44, 110),
                prompt: c(0xcb, 0xa6, 0xf7),
                error: c(0xf3, 0x8b, 0xa8),
            }
        }
        UiTheme::Dark => {
            let (text, blue) = (c(0xd4, 0xd4, 0xd4), c(0x56, 0x9c, 0xd6));
            Palette {
                colors: [
                    (text, false),
                    (c(0xc5, 0x86, 0xc0), false),
                    (c(0xce, 0x91, 0x78), false),
                    (c(0xb5, 0xce, 0xa8), false),
                    (blue, false),
                    (c(0x6a, 0x99, 0x55), false),
                    (c(0xdc, 0xdc, 0xaa), false),
                    (c(0x4e, 0xc9, 0xb0), false),
                    (blue, false),
                    (c(0xdc, 0xdc, 0xaa), false),
                    (text, false),
                    (text, false),
                    (blue, false),
                ],
                gutter: c(0x85, 0x85, 0x85),
                gutter_current: c(0xc6, 0xc6, 0xc6),
                current_line: Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 10),
                prompt: c(0xc5, 0x86, 0xc0),
                error: c(0xf4, 0x87, 0x71),
            }
        }
    }
}

/// `text` laid out in colour, unwrapped: Python read through a soft wrap is
/// Python with its indentation destroyed.
pub fn layout(text: &str, lang: Lang, palette: &Palette, font: egui::FontId) -> LayoutJob {
    let mut job = LayoutJob { text: text.to_owned(), ..Default::default() };
    job.wrap.max_width = f32::INFINITY;
    let spans = match lang {
        Lang::Python => python(text),
        Lang::Rust => rust(text),
        Lang::Plain => vec![(0, text.len(), Kind::Text)],
    };
    let mut at = 0;
    let push = |job: &mut LayoutJob, start: usize, end: usize, kind: Kind| {
        if start < end {
            let (color, italics) = palette.of(kind);
            job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(end),
                format: TextFormat { font_id: font.clone(), color, italics, ..Default::default() },
            });
        }
    };
    for (start, end, kind) in spans {
        push(&mut job, at, start, Kind::Text);
        push(&mut job, start, end, kind);
        at = end;
    }
    push(&mut job, at, text.len(), Kind::Text);
    job
}

/// `text` highlighted as `lang`, added to the end of `job`: a line of the
/// python tab after its prompt.
pub fn append(job: &mut LayoutJob, text: &str, lang: Lang, palette: &Palette, font: egui::FontId) {
    let offset = job.text.len();
    let piece = layout(text, lang, palette, font);
    job.text.push_str(&piece.text);
    for mut section in piece.sections {
        let range = section.byte_range;
        section.byte_range = egui::text::ByteIndex(range.start.0 + offset)..egui::text::ByteIndex(range.end.0 + offset);
        job.sections.push(section);
    }
}

const PY_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "case", "class", "continue", "def", "del", "elif", "else",
    "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "match", "nonlocal", "not",
    "or", "pass", "raise", "return", "try", "while", "with", "yield",
];
const PY_CONSTANTS: &[&str] = &["True", "False", "None", "Ellipsis", "NotImplemented", "__name__", "__file__"];
const RS_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static", "struct", "super",
    "trait", "type", "unsafe", "use", "where", "while",
];
const RS_PRIMITIVES: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
    "f32", "f64",
];

fn ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn ident_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && ident_byte(b[i]) {
        i += 1;
    }
    i
}

fn line_end(b: &[u8], i: usize) -> usize {
    b[i..].iter().position(|&c| c == b'\n').map_or(b.len(), |p| i + p)
}

/// The next byte that is not a space or a tab.
fn next_solid(b: &[u8], mut i: usize) -> Option<u8> {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    b.get(i).copied()
}

fn number_end(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
        // `1e-5`: the sign belongs to the exponent.
        if (b[i] == b'e' || b[i] == b'E') && i + 1 < b.len() && (b[i + 1] == b'-' || b[i + 1] == b'+') {
            i += 1;
        }
        i += 1;
    }
    i
}

/// What an ordinary name is: a type if it is CamelCase, a constant if it is
/// SHOUTED, a function if it is being called.
fn name_kind(word: &str, called: bool) -> Kind {
    let first_upper = word.as_bytes()[0].is_ascii_uppercase();
    let shouted = word.len() > 1 && word.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_');
    if shouted {
        Kind::Constant
    } else if first_upper {
        Kind::Type
    } else if called {
        Kind::Function
    } else {
        Kind::Text
    }
}

fn python(src: &str) -> Vec<(usize, usize, Kind)> {
    fn string_end(b: &[u8], i: usize) -> usize {
        let q = b[i];
        let triple = i + 2 < b.len() && b[i + 1] == q && b[i + 2] == q;
        let mut j = if triple { i + 3 } else { i + 1 };
        while j < b.len() {
            match b[j] {
                b'\\' => j += 2,
                c if c == q && (!triple || (j + 2 < b.len() && b[j + 1] == q && b[j + 2] == q)) => {
                    return if triple { j + 3 } else { j + 1 };
                }
                b'\n' if !triple => return j,
                _ => j += 1,
            }
        }
        b.len()
    }

    let b = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut after) = (0, "");
    let mut line_start = true;
    while i < b.len() {
        let c = b[i];
        let start = i;
        match c {
            b'#' => {
                i = line_end(b, i);
                out.push((start, i, Kind::Comment));
            }
            b'\'' | b'"' => {
                i = string_end(b, i);
                out.push((start, i, Kind::String));
            }
            b'@' if line_start => {
                i += 1;
                while i < b.len() && (ident_byte(b[i]) || b[i] == b'.') {
                    i += 1;
                }
                out.push((start, i, Kind::Decorator));
            }
            b'0'..=b'9' => {
                i = number_end(b, i);
                out.push((start, i, Kind::Number));
            }
            c if ident_byte(c) => {
                i = ident_end(b, i);
                let word = &src[start..i];
                // `f"..."`, `rb'...'`: a prefix, and the string it opens.
                if i < b.len() && (b[i] == b'"' || b[i] == b'\'') && word.len() <= 2
                    && word.bytes().all(|x| matches!(x.to_ascii_lowercase(), b'r' | b'b' | b'f' | b'u'))
                {
                    i = string_end(b, i);
                    out.push((start, i, Kind::String));
                    after = "";
                    line_start = false;
                    continue;
                }
                let kind = if PY_KEYWORDS.contains(&word) {
                    Kind::Keyword
                } else if PY_CONSTANTS.contains(&word) {
                    Kind::Constant
                } else if word == "self" || word == "cls" {
                    Kind::SelfWord
                } else if after == "def" {
                    Kind::Function
                } else if after == "class" {
                    Kind::Type
                } else {
                    name_kind(word, next_solid(b, i) == Some(b'('))
                };
                out.push((start, i, kind));
                after = if word == "def" || word == "class" { word } else { "" };
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^' | b'~' => {
                i += 1;
                while i < b.len() && matches!(b[i], b'=' | b'*' | b'/' | b'<' | b'>') {
                    i += 1;
                }
                out.push((start, i, Kind::Operator));
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b':' | b',' | b';' | b'.' => {
                i += 1;
                out.push((start, i, Kind::Punctuation));
            }
            _ => i += 1,
        }
        line_start = match c {
            b'\n' => true,
            b' ' | b'\t' => line_start,
            _ => false,
        };
    }
    out
}

fn rust(src: &str) -> Vec<(usize, usize, Kind)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let (mut i, mut after) = (0, "");
    while i < b.len() {
        let c = b[i];
        let start = i;
        match c {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = line_end(b, i);
                out.push((start, i, Kind::Comment));
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = src[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
                out.push((start, i, Kind::Comment));
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                out.push((start, i, Kind::String));
            }
            // `r"..."`, `r#"..."#`, `br"..."`.
            b'r' | b'b' if {
                let j = if c == b'b' && b.get(i + 1) == Some(&b'r') { i + 2 } else { i + 1 };
                matches!(b.get(j), Some(b'"') | Some(b'#')) && (c == b'r' || j == i + 2)
            } =>
            {
                i += if c == b'b' { 2 } else { 1 };
                let hashes = b[i..].iter().take_while(|&&x| x == b'#').count();
                i += hashes;
                if b.get(i) == Some(&b'"') {
                    let close = format!("\"{}", "#".repeat(hashes));
                    i = src[i + 1..].find(&close).map_or(b.len(), |p| i + 1 + p + close.len());
                }
                out.push((start, i, Kind::String));
            }
            b'\'' => {
                // A lifetime, `'a`, unless it closes as a character does.
                let name_end = ident_end(b, i + 1);
                if name_end > i + 1 && b.get(name_end) != Some(&b'\'') {
                    i = name_end;
                    out.push((start, i, Kind::Lifetime));
                } else {
                    i += 1;
                    while i < b.len() && b[i] != b'\'' && b[i] != b'\n' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                    i = (i + 1).min(b.len());
                    out.push((start, i, Kind::String));
                }
            }
            b'#' if matches!(b.get(i + 1), Some(b'[') | Some(b'!')) => {
                let mut depth = 0;
                while i < b.len() {
                    match b[i] {
                        b'[' => depth += 1,
                        b']' => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                out.push((start, i, Kind::Decorator));
            }
            b'0'..=b'9' => {
                i = number_end(b, i);
                out.push((start, i, Kind::Number));
            }
            c if ident_byte(c) => {
                i = ident_end(b, i);
                let word = &src[start..i];
                let kind = if RS_KEYWORDS.contains(&word) {
                    Kind::Keyword
                } else if word == "true" || word == "false" {
                    Kind::Constant
                } else if word == "self" || word == "Self" {
                    Kind::SelfWord
                } else if RS_PRIMITIVES.contains(&word) || after == "struct" || after == "enum" || after == "trait" {
                    Kind::Type
                } else if after == "fn" || b.get(i) == Some(&b'!') {
                    Kind::Function
                } else {
                    name_kind(word, next_solid(b, i) == Some(b'('))
                };
                out.push((start, i, kind));
                after = if matches!(word, "fn" | "struct" | "enum" | "trait") { word } else { "" };
            }
            b'+' | b'-' | b'*' | b'/' | b'%' | b'=' | b'<' | b'>' | b'!' | b'&' | b'|' | b'^' => {
                i += 1;
                while i < b.len() && matches!(b[i], b'=' | b'>' | b'&' | b'|') {
                    i += 1;
                }
                out.push((start, i, Kind::Operator));
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b':' | b',' | b';' | b'.' => {
                i += 1;
                out.push((start, i, Kind::Punctuation));
            }
            _ => i += 1,
        }
    }
    out
}

/// Byte offset of the `c`-th character.
fn byte_of(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

/// Character index of byte offset `b`.
fn char_of(s: &str, b: usize) -> usize {
    s[..b].chars().count()
}

/// Enter: the new line takes the indentation of the one it was split from,
/// and a level more after a line that opens a block -- `:` in Python, `{`
/// or `(` in Rust -- as VS Code does. `cursor` is just after the newline.
/// Returns where the cursor goes.
pub fn indent_new_line(text: &mut String, cursor: usize, lang: Lang) -> usize {
    let at = byte_of(text, cursor);
    if at == 0 || !text[..at].ends_with('\n') {
        return cursor;
    }
    let previous_start = text[..at - 1].rfind('\n').map_or(0, |p| p + 1);
    let previous = text[previous_start..at - 1].trim_end_matches('\r');
    let mut indent: String = previous.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let content = previous.trim_end();
    let opens = match lang {
        Lang::Python => content.ends_with(':') && !content.trim_start().starts_with('#'),
        Lang::Rust => content.ends_with('{') || content.ends_with('('),
        Lang::Plain => false,
    };
    if opens {
        indent.push_str("    ");
    }
    text.insert_str(at, &indent);
    cursor + indent.chars().count()
}

/// The first and last line the selection touches, as byte offsets of their
/// starts, and the selection's two ends as bytes.
fn lines_of(text: &str, range: CCursorRange) -> (Vec<usize>, usize, usize) {
    let (a, b) = (byte_of(text, range.primary.index.0), byte_of(text, range.secondary.index.0));
    let (lo, hi) = (a.min(b), a.max(b));
    let first = text[..lo].rfind('\n').map_or(0, |p| p + 1);
    let mut starts = vec![first];
    let mut at = first;
    while let Some(p) = text[at..hi].find('\n') {
        at += p + 1;
        if at < hi {
            starts.push(at);
        }
    }
    (starts, lo, hi)
}

/// Tab: spaces to the next multiple of four at the cursor, or, over a
/// selection that spans lines, four more at the start of each. Never a tab
/// character.
pub fn tab(text: &mut String, range: CCursorRange) -> CCursorRange {
    let (starts, lo, hi) = lines_of(text, range);
    if starts.len() == 1 && !text[lo..hi].contains('\n') {
        let column = char_of(text, lo) - char_of(text, starts[0]);
        let pad = 4 - column % 4;
        text.replace_range(lo..hi, &" ".repeat(pad));
        return CCursorRange::one(CCursor::new(char_of(text, lo) + pad));
    }
    for &start in starts.iter().rev() {
        text.insert_str(start, "    ");
    }
    shift(range, 4 * starts.len() as isize, 4)
}

/// Shift+Tab: up to four spaces off the start of each line the cursor or
/// the selection is on.
pub fn untab(text: &mut String, range: CCursorRange) -> CCursorRange {
    let (starts, _, _) = lines_of(text, range);
    let mut removed = 0;
    let mut first = 0;
    for (k, &start) in starts.iter().enumerate().rev() {
        let n = text[start..].bytes().take(4).take_while(|&c| c == b' ').count();
        text.replace_range(start..start + n, "");
        removed += n;
        if k == 0 {
            first = n;
        }
    }
    shift(range, -(removed as isize), -(first as isize))
}

/// Move a selection after its lines were indented: the end further along by
/// `total`, the start by `first` -- the change on its own line.
fn shift(range: CCursorRange, total: isize, first: isize) -> CCursorRange {
    let (p, s) = (range.primary.index.0 as isize, range.secondary.index.0 as isize);
    let (lo, hi) = if p <= s { (p, s) } else { (s, p) };
    let (lo, hi) = ((lo + first).max(0) as usize, (hi + total).max(0) as usize);
    // Each end stays the end it was: the one the cursor is at moves on.
    let (primary, secondary) = if p <= s { (lo, hi) } else { (hi, lo) };
    CCursorRange { primary: CCursor::new(primary), secondary: CCursor::new(secondary), h_pos: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(spans: &[(usize, usize, Kind)], src: &str) -> Vec<(String, Kind)> {
        spans.iter().map(|&(a, b, k)| (src[a..b].to_string(), k)).collect()
    }

    fn has(spans: &[(String, Kind)], text: &str, kind: Kind) -> bool {
        spans.iter().any(|(t, k)| t == text && *k == kind)
    }

    #[test]
    fn python_reads_as_vs_code_colours_it() {
        let src = "import numpy\n@app.callback\ndef before_render(sim, dt: float) -> None:\n    x = f\"{dt}\" + 'a#b'  # note\n    return self.Mesh(1e-5, MAX_N, True)\n\"\"\"doc\nmore\"\"\"\n";
        let s = kinds(&python(src), src);
        assert!(has(&s, "import", Kind::Keyword));
        assert!(has(&s, "@app.callback", Kind::Decorator));
        assert!(has(&s, "before_render", Kind::Function), "a def's name");
        assert!(has(&s, "f\"{dt}\"", Kind::String), "an f-string with its prefix");
        assert!(has(&s, "'a#b'", Kind::String), "a # inside a string is not a comment");
        assert!(has(&s, "# note", Kind::Comment));
        assert!(has(&s, "self", Kind::SelfWord));
        assert!(has(&s, "Mesh", Kind::Type));
        assert!(has(&s, "1e-5", Kind::Number), "the exponent's sign");
        assert!(has(&s, "MAX_N", Kind::Constant));
        assert!(has(&s, "True", Kind::Constant));
        assert!(has(&s, "None", Kind::Constant));
        assert!(has(&s, "\"\"\"doc\nmore\"\"\"", Kind::String), "a triple-quoted string over lines");
    }

    #[test]
    fn rust_reads_as_vs_code_colours_it() {
        let src = "#[derive(Debug)]\nfn main<'a>(x: &'a str) -> u32 {\n    let s = r#\"a\"b\"#; // c\n    println!(\"{}\", 'x', Self::new(0x1F));\n}\n";
        let s = kinds(&rust(src), src);
        assert!(has(&s, "#[derive(Debug)]", Kind::Decorator));
        assert!(has(&s, "fn", Kind::Keyword));
        assert!(has(&s, "main", Kind::Function));
        assert!(has(&s, "'a", Kind::Lifetime));
        assert!(has(&s, "u32", Kind::Type));
        assert!(has(&s, "r#\"a\"b\"#", Kind::String), "a raw string with a quote in it");
        assert!(has(&s, "// c", Kind::Comment));
        assert!(has(&s, "println", Kind::Function), "a macro");
        assert!(has(&s, "'x'", Kind::String), "a character, not a lifetime");
        assert!(has(&s, "Self", Kind::SelfWord));
        assert!(has(&s, "0x1F", Kind::Number));
    }

    /// Sections cover the text exactly once, in order -- what a `LayoutJob`
    /// needs -- whatever the text, a cut-off string included.
    #[test]
    fn a_layout_covers_every_byte_once() {
        let palette = palette(UiTheme::CatppuccinMocha);
        for (src, lang) in [("x = 'unterminated\ny = 2\n", Lang::Python), ("let é = \"ok\"; /* open", Lang::Rust), ("", Lang::Python)] {
            let job = layout(src, lang, &palette, egui::FontId::monospace(12.0));
            let mut at = 0;
            for s in &job.sections {
                assert_eq!(s.byte_range.start.0, at, "{src:?}");
                at = s.byte_range.end.0;
            }
            assert_eq!(at, src.len(), "{src:?}");
        }
    }

    #[test]
    fn enter_keeps_the_indent_and_opens_a_block() {
        let mut t = String::from("def f():\n");
        assert_eq!(indent_new_line(&mut t, 9, Lang::Python), 13);
        assert_eq!(t, "def f():\n    ");

        let mut t = String::from("    x = 1\n");
        assert_eq!(indent_new_line(&mut t, 10, Lang::Python), 14);
        assert_eq!(t, "    x = 1\n    ");

        let mut t = String::from("fn main() {\n}");
        indent_new_line(&mut t, 12, Lang::Rust);
        assert_eq!(t, "fn main() {\n    }");
    }

    #[test]
    fn tab_is_four_spaces_and_shift_tab_takes_them_back() {
        let mut t = String::from("ab");
        let r = tab(&mut t, CCursorRange::one(CCursor::new(1)));
        assert_eq!((t.as_str(), r.primary.index.0), ("a   b", 4), "to the next multiple of four");

        let mut t = String::from("a\nb\nc");
        let r = tab(&mut t, CCursorRange::two(CCursor::new(0), CCursor::new(3)));
        assert_eq!(t, "    a\n    b\nc", "each line the selection spans");
        let r = untab(&mut t, r);
        assert_eq!(t, "a\nb\nc");
        assert_eq!((r.primary.index.0, r.secondary.index.0), (3, 0), "the selection as it was, cursor end first");
    }
}
