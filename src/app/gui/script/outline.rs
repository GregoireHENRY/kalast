//! A script's outline, as VS Code's outline view shows one: its classes,
//! functions and methods and the variables beside them, nested as they are
//! in the text -- for Python and Rust, and a Markdown file's headings.
//!
//! Read off the text here rather than asked of the language server, so that
//! it is there at once, and without one: a line scan that knows strings and
//! comments well enough not to take a `def` in a docstring for a function.

/// What a symbol is, for its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Class,
    Function,
    Method,
    Variable,
    Field,
    Constant,
    Struct,
    Enum,
    Trait,
    Module,
    Implementation,
    Heading,
}

/// One row of the outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    /// Its line, from 0.
    pub line: usize,
    /// How deep it is nested: a method under its class is 1.
    pub depth: usize,
}

/// The outline of `text`, by its file's extension; empty for a kind this
/// does not read.
pub fn symbols(text: &str, path: &str) -> Vec<Symbol> {
    match path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("py" | "pyi") => python(text),
        Some("rs") => rust(text),
        Some("md" | "markdown") => markdown(text),
        _ => Vec::new(),
    }
}

/// Python: `class`, `def` and `async def`, nested by indentation, and the
/// names assigned in a module or a class body -- not in a function's.
fn python(text: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    // The blocks open above the line: (indent, is it a class, depth).
    let mut open: Vec<(usize, bool, usize)> = Vec::new();
    // Inside a triple-quoted string, with its quotes.
    let mut in_string: Option<&str> = None;
    // Inside brackets left open: a continuation line, not a statement.
    let mut brackets = 0i32;
    for (line, raw) in text.lines().enumerate() {
        if let Some(q) = in_string {
            if count_outside(raw, q) % 2 == 1 {
                in_string = None;
            }
            continue;
        }
        let code = raw.trim_start();
        let continued = brackets > 0;
        brackets += bracket_balance(raw);
        if code.is_empty() || code.starts_with('#') || continued {
            continue;
        }
        // A triple quote left open on this line opens a string to come.
        for q in ["\"\"\"", "'''"] {
            if count_outside(raw, q) % 2 == 1 {
                in_string = Some(q);
            }
        }
        let indent = raw.len() - code.len();
        while open.last().is_some_and(|&(i, _, _)| i >= indent) {
            open.pop();
        }
        let depth = open.last().map_or(0, |&(_, _, d)| d + 1);
        let in_class = open.last().is_some_and(|&(_, class, _)| class);
        let in_function = open.last().is_some_and(|&(_, class, _)| !class);
        let code = code.strip_prefix("async ").unwrap_or(code);
        if let Some(rest) = code.strip_prefix("class ") {
            out.push(Symbol { name: ident(rest), kind: Kind::Class, line, depth });
            open.push((indent, true, depth));
        } else if let Some(rest) = code.strip_prefix("def ") {
            let kind = if in_class { Kind::Method } else { Kind::Function };
            out.push(Symbol { name: ident(rest), kind, line, depth });
            open.push((indent, false, depth));
        } else if !in_function {
            if let Some(name) = assigned(code) {
                let kind = if in_class {
                    Kind::Field
                } else if name.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                    Kind::Constant
                } else {
                    Kind::Variable
                };
                out.push(Symbol { name, kind, line, depth });
            }
        }
    }
    out
}

/// The name a statement assigns, `x = ...` or `x: int = ...`, and not a
/// comparison, an augmented assignment or an attribute's.
fn assigned(code: &str) -> Option<String> {
    let name = ident(code);
    if name.is_empty() || matches!(name.as_str(), "if" | "for" | "while" | "return" | "import" | "from" | "with" | "print") {
        return None;
    }
    let rest = code[name.len()..].trim_start();
    let is_assignment = (rest.starts_with('=') && !rest.starts_with("=="))
        || (rest.starts_with(':') && !rest.starts_with(":=") && rest.contains('='));
    is_assignment.then_some(name)
}

/// How many times `needle` occurs in `line` outside a `#` comment.
fn count_outside(line: &str, needle: &str) -> usize {
    let code = match line.find('#') {
        // A `#` inside quotes is not a comment; this only has to be good
        // enough for triple quotes, which a comment rarely holds.
        Some(i) if !line[..i].contains('"') && !line[..i].contains('\'') => &line[..i],
        _ => line,
    };
    code.matches(needle).count()
}

/// Brackets a line opens less those it closes, strings and comments aside.
fn bracket_balance(line: &str) -> i32 {
    let mut n = 0;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == '\\' {
                    chars.next();
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '#' => break,
                '"' | '\'' => quote = Some(c),
                '(' | '[' | '{' => n += 1,
                ')' | ']' | '}' => n -= 1,
                _ => {}
            },
        }
    }
    n
}

/// The identifier `s` starts with.
fn ident(s: &str) -> String {
    s.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()
}

/// Rust: items -- `fn`, `struct`, `enum`, `trait`, `impl`, `mod`, `const`,
/// `static`, `type`, `macro_rules!` -- nested by braces, strings and
/// comments stepped over. A `fn` in an `impl` or a `trait` is a method.
fn rust(text: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    // Each open brace, with the kind of the item it holds the items of --
    // an `impl`, a `trait`, a `mod` -- or `None` for any other: a body.
    let mut braces: Vec<Option<Kind>> = Vec::new();
    let mut in_block_comment = 0usize;
    // An item seen whose `{` has not come yet, and what its body holds.
    let mut pending: Option<Option<Kind>> = None;
    for (line, raw) in text.lines().enumerate() {
        let code = strip_rust(raw, &mut in_block_comment);
        let trimmed = code.trim_start();
        let depth = braces.iter().filter(|b| b.is_some()).count();
        let in_body = braces.last().is_some_and(Option::is_none);
        let in_impl = braces.iter().rev().find_map(|b| *b).is_some_and(|k| matches!(k, Kind::Implementation | Kind::Trait));
        if !in_body && !trimmed.starts_with('#') {
            if let Some((name, kind, holds)) = rust_item(trimmed, in_impl) {
                out.push(Symbol { name, kind, line, depth });
                pending = Some(holds);
            }
        }
        for c in code.chars() {
            match c {
                '{' => braces.push(pending.take().flatten()),
                '}' => {
                    braces.pop();
                }
                ';' => pending = None,
                _ => {}
            }
        }
    }
    out
}

/// The item a line of Rust begins, its visibility and qualifiers aside:
/// `(name, kind, what its body holds the items of)`.
fn rust_item(line: &str, in_impl: bool) -> Option<(String, Kind, Option<Kind>)> {
    let mut s = line.trim_start();
    // `pub(crate)`, `pub`, `async`, `unsafe`, `const` before `fn`...
    loop {
        let next = if let Some(r) = s.strip_prefix("pub(") {
            r.find(')').map(|i| &r[i + 1..])
        } else {
            ["pub ", "async ", "unsafe ", "default ", "extern \"C\" "]
                .iter()
                .find_map(|q| s.strip_prefix(q))
                .or_else(|| s.strip_prefix("const ").filter(|r| r.trim_start().starts_with("fn ")))
        };
        match next {
            Some(r) => s = r.trim_start(),
            None => break,
        }
    }
    let word = |w: &str| s.strip_prefix(w).filter(|r| r.starts_with(char::is_whitespace)).map(str::trim_start);
    let item = |r: &str, kind: Kind, holds: Option<Kind>| Some((ident(r), kind, holds));
    if let Some(r) = word("fn") {
        return item(r, if in_impl { Kind::Method } else { Kind::Function }, None);
    }
    for (w, kind, holds) in [
        ("struct", Kind::Struct, None),
        ("enum", Kind::Enum, None),
        ("union", Kind::Struct, None),
        ("type", Kind::Struct, None),
        ("trait", Kind::Trait, Some(Kind::Trait)),
        ("mod", Kind::Module, Some(Kind::Module)),
        ("const", Kind::Constant, None),
    ] {
        if let Some(r) = word(w) {
            return item(r, kind, holds);
        }
    }
    if let Some(r) = word("static") {
        return item(r.strip_prefix("mut ").unwrap_or(r), Kind::Constant, None);
    }
    if let Some(r) = s.strip_prefix("macro_rules!") {
        return item(r, Kind::Function, None);
    }
    if let Some(r) = s.strip_prefix("impl").filter(|r| r.starts_with(char::is_whitespace) || r.starts_with('<')) {
        // `impl<T> Trait for Type {`: named as VS Code names it.
        let head = r.split('{').next().unwrap_or(r).trim();
        let head = if head.starts_with('<') { skip_generics(head) } else { head };
        let name = head.split(" where ").next().unwrap_or(head).split_whitespace().collect::<Vec<_>>().join(" ");
        return Some((name, Kind::Implementation, Some(Kind::Implementation)));
    }
    None
}

/// `s` after the `<...>` it starts with.
fn skip_generics(s: &str) -> &str {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return s[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    s
}

/// A line of Rust without its strings, chars and comments -- their text
/// could hold braces -- and whether a block comment runs on.
fn strip_rust(line: &str, in_block_comment: &mut usize) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if *in_block_comment > 0 {
            if c == '*' && next == Some('/') {
                *in_block_comment -= 1;
                i += 2;
            } else if c == '/' && next == Some('*') {
                *in_block_comment += 1;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        match (c, next) {
            ('/', Some('/')) => break,
            ('/', Some('*')) => {
                *in_block_comment += 1;
                i += 2;
            }
            ('"', _) => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
                out.push_str("\"\"");
            }
            // A char, not a lifetime: `'x'`, `'\n'`.
            ('\'', _) if chars.get(i + 2) == Some(&'\'') || (next == Some('\\') && chars.get(i + 3) == Some(&'\'')) => {
                i += if next == Some('\\') { 4 } else { 3 };
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Markdown: its `#` headings, outside fenced code.
fn markdown(text: &str) -> Vec<Symbol> {
    let mut out = Vec::new();
    let mut fenced = false;
    for (line, raw) in text.lines().enumerate() {
        let t = raw.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let level = t.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&level) && t[level..].starts_with(' ') {
            let name = t[level..].trim().trim_end_matches('#').trim().to_string();
            out.push(Symbol { name, kind: Kind::Heading, line, depth: level - 1 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(symbols: &[Symbol]) -> Vec<(usize, &str, Kind, usize)> {
        symbols.iter().map(|s| (s.depth, s.name.as_str(), s.kind, s.line)).collect()
    }

    #[test]
    fn a_python_script_is_outlined_as_vs_code_outlines_it() {
        let text = "\
import numpy
RATE = 2
app = App()

class Body:
    \"\"\"A body.

    def not_this(): a docstring's line
    \"\"\"
    mass: float = 1.0

    def spin(self, dt):
        x = dt * 2
        def inner():
            pass

async def run(
    a,
    b=3,
):
    total = a + b
    return total

if __name__ == \"__main__\":
    run(1)
";
        assert_eq!(
            rows(&python(text)),
            [
                (0, "RATE", Kind::Constant, 1),
                (0, "app", Kind::Variable, 2),
                (0, "Body", Kind::Class, 4),
                (1, "mass", Kind::Field, 9),
                (1, "spin", Kind::Method, 11),
                (2, "inner", Kind::Function, 13),
                (0, "run", Kind::Function, 16),
            ]
        );
    }

    #[test]
    fn a_rust_file_is_outlined_by_its_items() {
        let text = "\
use kalast::*;
/// A body { not a brace }
pub struct Body {
    mass: f64,
}

impl Body {
    pub fn new() -> Self {
        let s = \"}\";
        Self { mass: 1.0 }
    }
    fn spin(&mut self) {}
}

impl<T: Clone> Trait for Wrapper<T> {}

pub(crate) const RATE: f64 = 2.0;
mod inner {
    fn helper() {}
}
fn main() {
    let c = '{';
}
";
        assert_eq!(
            rows(&rust(text)),
            [
                (0, "Body", Kind::Struct, 2),
                (0, "Body", Kind::Implementation, 6),
                (1, "new", Kind::Method, 7),
                (1, "spin", Kind::Method, 11),
                (0, "Trait for Wrapper<T>", Kind::Implementation, 14),
                (0, "RATE", Kind::Constant, 16),
                (0, "inner", Kind::Module, 17),
                (1, "helper", Kind::Function, 18),
                (0, "main", Kind::Function, 20),
            ]
        );
    }

    #[test]
    fn a_markdown_file_is_outlined_by_its_headings() {
        let text = "# Title\n\nText\n\n## Part\n\n```sh\n# not a heading\n```\n### Detail ##\n#nope\n";
        assert_eq!(
            rows(&markdown(text)),
            [(0, "Title", Kind::Heading, 0), (1, "Part", Kind::Heading, 4), (2, "Detail", Kind::Heading, 9)]
        );
    }

    #[test]
    fn the_kind_of_file_decides() {
        assert!(!symbols("def f(): pass", "a.py").is_empty());
        assert!(symbols("def f(): pass", "a.txt").is_empty());
    }
}
