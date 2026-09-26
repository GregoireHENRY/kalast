//! The Markdown a language server writes -- hovers, completion docs,
//! signatures -- as VS Code shows it: code blocks highlighted as code, rules
//! between sections, and inline `code`, **bold** and *italics* in the
//! paragraphs. Enough of Markdown for what servers send; not a renderer for
//! documents.

use super::super::code::{self, Lang, Palette};
use super::lsp::Markup;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

/// One block of a document, in order.
#[derive(Debug, PartialEq)]
enum Block {
    Code(Lang, String),
    Rule,
    Heading(String),
    Paragraph(String),
    Bullet(String),
}

fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let flush = |paragraph: &mut Vec<String>, out: &mut Vec<Block>| {
        if !paragraph.is_empty() {
            out.push(Block::Paragraph(paragraph.join(" ")));
            paragraph.clear();
        }
    };
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(fence) = trimmed.strip_prefix("```") {
            flush(&mut paragraph, &mut out);
            let lang = match fence.trim() {
                "python" | "py" | "python3" | "pycon" => Lang::Python,
                "rust" | "rs" => Lang::Rust,
                _ => Lang::Plain,
            };
            let mut body = Vec::new();
            for line in lines.by_ref() {
                if line.trim_start().starts_with("```") {
                    break;
                }
                body.push(line);
            }
            out.push(Block::Code(lang, body.join("\n")));
        } else if trimmed.is_empty() {
            flush(&mut paragraph, &mut out);
        } else if !paragraph.is_empty()
            && trimmed.len() >= 3
            && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'))
        {
            // A line of `=` or `-` under text underlines it: a heading, as
            // numpy's docstrings title their modules.
            out.push(Block::Heading(paragraph.join(" ")));
            paragraph.clear();
        } else if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') && trimmed.len() >= 3 {
            flush(&mut paragraph, &mut out);
            out.push(Block::Rule);
        } else if trimmed.starts_with('#') {
            flush(&mut paragraph, &mut out);
            out.push(Block::Heading(trimmed.trim_start_matches('#').trim().to_string()));
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            flush(&mut paragraph, &mut out);
            out.push(Block::Bullet(item.to_string()));
        } else if line.starts_with("    ") && paragraph.is_empty() {
            // An indented line: code, as docstrings indent their examples.
            // Its neighbours join it below.
            out.push(Block::Code(Lang::Plain, line[4..].to_string()));
        } else {
            // A line ending in two spaces or a backslash breaks; otherwise the
            // lines of a paragraph run together, as Markdown reads them.
            if line.ends_with("  ") || line.ends_with('\\') {
                paragraph.push(format!("{}\n", trimmed.trim_end_matches('\\')));
            } else {
                paragraph.push(trimmed.to_string());
            }
        }
    }
    flush(&mut paragraph, &mut out);
    // Adjacent indented lines, read one at a time above, become one block.
    let mut merged: Vec<Block> = Vec::new();
    for block in out {
        match (merged.last_mut(), block) {
            (Some(Block::Code(Lang::Plain, prev)), Block::Code(Lang::Plain, next)) => {
                prev.push('\n');
                prev.push_str(&next);
            }
            (_, block) => merged.push(block),
        }
    }
    merged
}

/// A paragraph with its inline styles, as one wrapped layout.
fn inline(text: &str, body: &FontId, mono: &FontId, color: Color32, strong: Color32, code_bg: Color32, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let plain = TextFormat { font_id: body.clone(), color, ..Default::default() };
    let mut run = String::new();
    let mut format = plain.clone();
    let push = |job: &mut LayoutJob, run: &mut String, format: &TextFormat| {
        if !run.is_empty() {
            job.append(run, 0.0, format.clone());
            run.clear();
        }
    };
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let (mut bold, mut italic) = (false, false);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() => {
                run.push(chars[i + 1]);
                i += 2;
                continue;
            }
            '`' => {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == '`') {
                    push(&mut job, &mut run, &format);
                    let code: String = chars[i + 1..i + 1 + end].iter().collect();
                    job.append(
                        &code,
                        0.0,
                        TextFormat { font_id: mono.clone(), color: strong, background: code_bg, ..Default::default() },
                    );
                    i += end + 2;
                    continue;
                }
            }
            '*' | '_' => {
                let double = i + 1 < chars.len() && chars[i + 1] == c;
                // `snake_case` is not emphasis: an underscore inside a word
                // is a letter.
                let inside_word = c == '_'
                    && i > 0
                    && chars[i - 1].is_alphanumeric()
                    && chars.get(i + if double { 2 } else { 1 }).is_some_and(|n| n.is_alphanumeric());
                if !inside_word {
                    push(&mut job, &mut run, &format);
                    if double {
                        bold = !bold;
                        i += 2;
                    } else {
                        italic = !italic;
                        i += 1;
                    }
                    format = TextFormat { italics: italic, color: if bold { strong } else { color }, ..plain.clone() };
                    continue;
                }
            }
            '[' => {
                // `[text](url)`: the text.
                if let Some(close) = chars[i..].iter().position(|&c| c == ']') {
                    let close = i + close;
                    if chars.get(close + 1) == Some(&'(') {
                        if let Some(end) = chars[close..].iter().position(|&c| c == ')') {
                            push(&mut job, &mut run, &format);
                            let label: String = chars[i + 1..close].iter().collect();
                            job.append(&label, 0.0, TextFormat { underline: egui::Stroke::new(1.0, strong), ..format.clone() });
                            i = close + end + 1;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        run.push(c);
        i += 1;
    }
    push(&mut job, &mut run, &format);
    job
}

/// Draw `markup`, at most `width` wide.
pub fn show(ui: &mut egui::Ui, markup: &Markup, palette: &Palette, width: f32) {
    let body = egui::TextStyle::Body.resolve(ui.style());
    let mono = FontId::monospace(12.5);
    let visuals = ui.visuals().clone();
    let color = visuals.text_color();
    let strong = visuals.strong_text_color();
    let code_bg = visuals.code_bg_color;
    ui.spacing_mut().item_spacing.y = 4.0;
    if markup.plain {
        ui.add(egui::Label::new(egui::RichText::new(markup.text.trim()).font(body).color(color)).wrap());
        return;
    }
    for block in blocks(&markup.text) {
        match block {
            Block::Code(lang, text) => {
                let mut job = code::layout(&text, lang, palette, mono.clone());
                job.wrap.max_width = width;
                ui.add(egui::Label::new(job).selectable(true));
            }
            Block::Rule => {
                ui.add_space(1.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
                ui.painter().hline(rect.x_range(), rect.center().y, visuals.widgets.noninteractive.bg_stroke);
                ui.add_space(1.0);
            }
            Block::Heading(text) => {
                ui.add(egui::Label::new(inline(&text, &body, &mono, strong, strong, code_bg, width)).selectable(true));
            }
            Block::Paragraph(text) => {
                ui.add(egui::Label::new(inline(&text, &body, &mono, color, strong, code_bg, width)).selectable(true));
            }
            Block::Bullet(text) => {
                ui.add(
                    egui::Label::new(inline(&format!("\u{2022} {text}"), &body, &mono, color, strong, code_bg, width))
                        .selectable(true),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What pyright sends for a function: its signature as a code block, a
    /// rule, then the docstring.
    #[test]
    fn a_hover_splits_into_code_rule_and_text() {
        let text = "```python\n(function) def f(x: int) -> str\n```\n---\nTurns *x* into\n`str`.\n\nSecond.";
        assert_eq!(
            blocks(text),
            vec![
                Block::Code(Lang::Python, "(function) def f(x: int) -> str".into()),
                Block::Rule,
                Block::Paragraph("Turns *x* into `str`.".into()),
                Block::Paragraph("Second.".into()),
            ]
        );
    }

    #[test]
    fn an_underline_makes_a_heading() {
        assert_eq!(
            blocks("numpy.linalg\n============\n\nLinear algebra.\n\n---\nafter"),
            vec![
                Block::Heading("numpy.linalg".into()),
                Block::Paragraph("Linear algebra.".into()),
                Block::Rule,
                Block::Paragraph("after".into()),
            ]
        );
    }

    #[test]
    fn indented_lines_are_one_code_block() {
        let text = "Example:\n\n    a = f(1)\n    b = f(2)\n\nDone.";
        assert_eq!(
            blocks(text),
            vec![
                Block::Paragraph("Example:".into()),
                Block::Code(Lang::Plain, "a = f(1)\nb = f(2)".into()),
                Block::Paragraph("Done.".into()),
            ]
        );
    }

    #[test]
    fn inline_styles_become_sections() {
        let f = FontId::proportional(13.0);
        let job = inline(
            "a `b` **c** snake_case \\_x [link](http://x)",
            &f,
            &f,
            Color32::WHITE,
            Color32::RED,
            Color32::BLACK,
            100.0,
        );
        assert_eq!(job.text, "a b c snake_case _x link");
        assert!(job.sections.len() >= 5, "{:?}", job.sections.len());
    }
}
