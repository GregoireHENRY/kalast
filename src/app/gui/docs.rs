//! The documentation tab: kalast's README and references -- the Python API,
//! the config, the controls, the changelog, and the READMEs of `res/` and
//! `examples/` -- rendered in the window, as VS Code previews Markdown.
//!
//! Compiled in, not read from disk: a wheel installed with pip has no
//! `docs/` beside it, and what is shown is then always what this build does.
//! Parsed once, with pulldown-cmark, into blocks, of which only those in view
//! are laid out and drawn, the others' heights remembered: `CONFIG.md` alone
//! is some six hundred of them, and this runs every frame the tab is open,
//! beside a simulation it is not to slow down.
//!
//! A link goes where it points: another page or a heading -- by the anchor
//! GitHub gives the heading, so one link works in both places -- a script or
//! a mesh, opened as the files tab opens one, a folder, shown in the files
//! tab, and anything else to the browser, a file of the repository that is
//! not on this disk to GitHub.

use super::code::{self, Lang};
use super::theme::{self, palette};
use super::{icons, INDENT, ROW, TWISTIE};
use crate::app::config::UiTheme;
use egui::text::{LayoutJob, TextFormat};
use egui::{pos2, vec2, Color32, FontId, Galley, Rect, Sense, Stroke};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

/// A page: where it sits in the repository, which is what the links between
/// pages are written against, and its text.
struct Source {
    path: &'static str,
    text: &'static str,
}

/// The pages, in the order the tab lists them: the README first, which the
/// tab opens on.
const SOURCES: [Source; 7] = [
    Source { path: "README.md", text: include_str!("../../../README.md") },
    Source { path: "docs/API.md", text: include_str!("../../../docs/API.md") },
    Source { path: "docs/CONFIG.md", text: include_str!("../../../docs/CONFIG.md") },
    Source { path: "docs/CONTROLS.md", text: include_str!("../../../docs/CONTROLS.md") },
    Source { path: "CHANGELOG.md", text: include_str!("../../../CHANGELOG.md") },
    Source { path: "res/README.md", text: include_str!("../../../res/README.md") },
    Source { path: "examples/README.md", text: include_str!("../../../examples/README.md") },
];

/// Body text's size, and its lines' spacing as a multiple of it: VS Code's
/// preview reads at 14 and 1.6, where egui's own 1.2 is cramped for a page.
const BODY: f32 = 14.0;
const SPACING: f32 = 1.55;
/// The column's widest, and the room around it.
const WIDTH: f32 = 860.0;
const MARGIN: f32 = 28.0;
/// How far a list item's text sits in from where its bullet does.
const ITEM: f32 = 22.0;
/// A table column is not cut down narrower than this.
const MIN_COLUMN: f32 = 64.0;
/// Between a picture floated right and the text beside it.
const FLOAT_GAP: f32 = 16.0;

/// What a span of text is, beyond its words, as bits.
const STRONG: u8 = 1;
const EMPH: u8 = 2;
const CODE: u8 = 4;
const STRIKE: u8 = 8;

/// A run of text in one style, and the link it is part of, by its index in
/// the page's links.
#[derive(Debug, Clone, PartialEq)]
struct Span {
    text: String,
    style: u8,
    link: Option<usize>,
}

/// One block of a page.
#[derive(Debug, PartialEq)]
enum Block {
    Heading { level: u8, text: Vec<Span> },
    Paragraph(Vec<Span>),
    Code { lang: Lang, text: String },
    Rule,
    Quote(Vec<Block>),
    /// Each item its own blocks; numbered from `start` when there is one.
    List { start: Option<u64>, items: Vec<Vec<Block>> },
    Table { align: Vec<Alignment>, head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
    /// A picture placed with an HTML `<img>`, as READMEs place theirs: its
    /// path as written, its width if the tag gives one, and whether it
    /// floats to the right of what follows.
    Image { src: String, width: Option<f32>, right: bool },
}

/// A heading at a page's top level: an outline row, and a place to link to.
#[derive(Debug)]
struct Heading {
    level: u8,
    text: String,
    /// The anchor GitHub gives it -- `what-a-mesh-carries` -- which is what
    /// the pages' links are written with.
    anchor: String,
    /// Which of the page's blocks it is.
    block: usize,
}

/// A page, parsed.
struct Page {
    /// Its first `#` heading, named on hover in the page list.
    title: String,
    blocks: Vec<Block>,
    /// Every link's destination, as written.
    links: Vec<String>,
    headings: Vec<Heading>,
}

/// What the parser has open, innermost last.
enum Open {
    Page(Vec<Block>),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Vec<Block>>),
    Item(Vec<Block>),
    Table(Vec<Alignment>, Vec<Vec<Span>>, Vec<Vec<Vec<Span>>>),
    Row(Vec<Vec<Span>>),
}

/// pulldown-cmark's events, gathered into blocks.
struct Builder {
    open: Vec<Open>,
    /// The text being gathered: a paragraph's, a heading's, a cell's.
    inline: Option<Vec<Span>>,
    /// That text is a tight list item's, which comes with no paragraph
    /// around it, and becomes one when the item's next block starts.
    implicit: bool,
    code: Option<(Lang, String)>,
    style: u8,
    link: Option<usize>,
    links: Vec<String>,
    /// Inside an image or an HTML block, whose text is not shown.
    skip: usize,
    headings: Vec<Heading>,
    /// How many headings have had each anchor, for GitHub's `-1`, `-2`.
    anchors: HashMap<String, usize>,
}

impl Builder {
    /// Where a block goes: the innermost page, quote or list item.
    fn blocks(&mut self) -> &mut Vec<Block> {
        self.open
            .iter_mut()
            .rev()
            .find_map(|open| match open {
                Open::Page(blocks) | Open::Quote(blocks) | Open::Item(blocks) => Some(blocks),
                _ => None,
            })
            .expect("the page is open until the end")
    }

    /// A tight item's text, made a paragraph before whatever comes next.
    fn settle(&mut self) {
        if std::mem::take(&mut self.implicit) {
            if let Some(spans) = self.inline.take() {
                self.blocks().push(Block::Paragraph(spans));
            }
        }
    }

    fn text(&mut self, text: &str, style: u8) {
        if let Some((_, code)) = &mut self.code {
            code.push_str(text);
            return;
        }
        if self.skip > 0 {
            return;
        }
        if self.inline.is_none() {
            self.implicit = true;
            self.inline = Some(Vec::new());
        }
        let link = self.link;
        let spans = self.inline.as_mut().expect("made above");
        match spans.last_mut() {
            Some(last) if last.style == style && last.link == link => last.text.push_str(text),
            _ => spans.push(Span { text: text.to_owned(), style, link }),
        }
    }

    fn heading(&mut self, level: u8) {
        let text = self.inline.take().unwrap_or_default();
        let plain: String = text.iter().map(|s| s.text.as_str()).collect();
        let base = slug(&plain);
        let seen = self.anchors.entry(base.clone()).or_insert(0);
        let anchor = if *seen == 0 { base } else { format!("{base}-{seen}") };
        *seen += 1;
        if let [Open::Page(blocks)] = self.open.as_slice() {
            self.headings.push(Heading { level, text: plain, anchor, block: blocks.len() });
        }
        self.blocks().push(Block::Heading { level, text });
    }
}

/// GitHub's anchor for a heading: lower case, spaces to hyphens, and
/// anything but letters, digits, `-` and `_` dropped.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.trim().chars() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            out.extend(c.to_lowercase());
        } else if c == ' ' {
            out.push('-');
        }
    }
    out
}

/// The picture an `<img>` tag in `html` places, if there is one.
fn img(html: &str) -> Option<Block> {
    let tag = &html[html.find("<img")?..];
    let tag = &tag[..tag.find('>').map_or(tag.len(), |end| end + 1)];
    Some(Block::Image {
        src: attr(tag, "src")?,
        width: attr(tag, "width").and_then(|w| w.trim_end_matches("px").trim().parse().ok()),
        right: attr(tag, "align").is_some_and(|a| a.eq_ignore_ascii_case("right")),
    })
}

/// An attribute's value in an HTML tag, quoted or not.
fn attr(tag: &str, name: &str) -> Option<String> {
    for (i, _) in tag.match_indices(name) {
        let rest = &tag[i + name.len()..];
        if !tag[..i].ends_with(char::is_whitespace) || !rest.starts_with('=') {
            continue;
        }
        let value = &rest[1..];
        return match value.chars().next()? {
            quote @ ('"' | '\'') => value[1..].split(quote).next().map(str::to_owned),
            _ => value.split(|c: char| c.is_whitespace() || c == '>').next().map(str::to_owned),
        };
    }
    None
}

/// The pictures the pages show, by their path in the repository, compiled
/// in as the pages are. The one there is: the logo at the top of the README.
fn picture(path: &str) -> Option<&'static [u8]> {
    match path {
        "src/app/gui/assets/kalast-256.png" => Some(super::LOGO),
        _ => None,
    }
}

/// Every picture's source in `blocks`, those in lists and quotes too.
fn images<'a>(blocks: &'a [Block], found: &mut Vec<&'a str>) {
    for block in blocks {
        match block {
            Block::Image { src, .. } => found.push(src),
            Block::Quote(inner) => images(inner, found),
            Block::List { items, .. } => items.iter().for_each(|item| images(item, found)),
            _ => {}
        }
    }
}

/// A fence's language, for its colours: Python and Rust as the editor draws
/// them, anything else plain.
fn lang_of(info: &str) -> Lang {
    match info.split([',', ' ']).next().unwrap_or("").trim() {
        "python" | "py" | "python3" | "pycon" | "pyi" => Lang::Python,
        "rust" | "rs" => Lang::Rust,
        _ => Lang::Plain,
    }
}

fn parse(text: &str) -> Page {
    // A Windows checkout's line ends, which would otherwise ride along into
    // the code blocks.
    let text = text.replace("\r\n", "\n");
    let mut b = Builder {
        open: vec![Open::Page(Vec::new())],
        inline: None,
        implicit: false,
        code: None,
        style: 0,
        link: None,
        links: Vec::new(),
        skip: 0,
        headings: Vec::new(),
        anchors: HashMap::new(),
    };
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(&text, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph | Tag::Heading { .. } => {
                    b.settle();
                    b.inline = Some(Vec::new());
                }
                Tag::BlockQuote(_) => {
                    b.settle();
                    b.open.push(Open::Quote(Vec::new()));
                }
                Tag::CodeBlock(kind) => {
                    b.settle();
                    let lang = match &kind {
                        CodeBlockKind::Fenced(info) => lang_of(info),
                        CodeBlockKind::Indented => Lang::Plain,
                    };
                    b.code = Some((lang, String::new()));
                }
                Tag::List(start) => {
                    b.settle();
                    b.open.push(Open::List(start, Vec::new()));
                }
                Tag::Item => b.open.push(Open::Item(Vec::new())),
                Tag::Table(align) => {
                    b.settle();
                    b.open.push(Open::Table(align, Vec::new(), Vec::new()));
                }
                Tag::TableHead | Tag::TableRow => b.open.push(Open::Row(Vec::new())),
                Tag::TableCell => b.inline = Some(Vec::new()),
                Tag::Emphasis => b.style |= EMPH,
                Tag::Strong => b.style |= STRONG,
                Tag::Strikethrough => b.style |= STRIKE,
                Tag::Link { dest_url, .. } => {
                    b.links.push(dest_url.into_string());
                    b.link = Some(b.links.len() - 1);
                }
                Tag::Image { .. } | Tag::HtmlBlock => b.skip += 1,
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if let Some(spans) = b.inline.take() {
                        b.blocks().push(Block::Paragraph(spans));
                    }
                }
                TagEnd::Heading(level) => b.heading(level as u8),
                TagEnd::BlockQuote(_) => {
                    b.settle();
                    if let Some(Open::Quote(blocks)) = b.open.pop() {
                        b.blocks().push(Block::Quote(blocks));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some((lang, mut text)) = b.code.take() {
                        while text.ends_with('\n') {
                            text.pop();
                        }
                        b.blocks().push(Block::Code { lang, text });
                    }
                }
                TagEnd::List(_) => {
                    if let Some(Open::List(start, items)) = b.open.pop() {
                        b.blocks().push(Block::List { start, items });
                    }
                }
                TagEnd::Item => {
                    b.settle();
                    if let Some(Open::Item(blocks)) = b.open.pop() {
                        if let Some(Open::List(_, items)) = b.open.last_mut() {
                            items.push(blocks);
                        }
                    }
                }
                TagEnd::TableCell => {
                    let cell = b.inline.take().unwrap_or_default();
                    if let Some(Open::Row(cells)) = b.open.last_mut() {
                        cells.push(cell);
                    }
                }
                TagEnd::TableHead => {
                    if let Some(Open::Row(cells)) = b.open.pop() {
                        if let Some(Open::Table(_, head, _)) = b.open.last_mut() {
                            *head = cells;
                        }
                    }
                }
                TagEnd::TableRow => {
                    if let Some(Open::Row(cells)) = b.open.pop() {
                        if let Some(Open::Table(_, _, rows)) = b.open.last_mut() {
                            rows.push(cells);
                        }
                    }
                }
                TagEnd::Table => {
                    if let Some(Open::Table(align, head, rows)) = b.open.pop() {
                        b.blocks().push(Block::Table { align, head, rows });
                    }
                }
                TagEnd::Emphasis => b.style &= !EMPH,
                TagEnd::Strong => b.style &= !STRONG,
                TagEnd::Strikethrough => b.style &= !STRIKE,
                TagEnd::Link => b.link = None,
                TagEnd::Image | TagEnd::HtmlBlock => b.skip = b.skip.saturating_sub(1),
                _ => {}
            },
            Event::Text(text) => b.text(&text, b.style),
            Event::Code(text) => b.text(&text, b.style | CODE),
            Event::SoftBreak => b.text(" ", b.style),
            Event::HardBreak => b.text("\n", b.style),
            Event::Rule => {
                b.settle();
                b.blocks().push(Block::Rule);
            }
            Event::TaskListMarker(done) => b.text(if done { "[x] " } else { "[ ] " }, b.style | CODE),
            // An HTML block's text is not shown, but a picture in it is.
            Event::Html(html) => {
                if let Some(image) = img(&html) {
                    b.settle();
                    b.blocks().push(image);
                }
            }
            _ => {}
        }
    }
    b.settle();
    let title = b.headings.iter().find(|h| h.level == 1).map(|h| h.text.clone()).unwrap_or_default();
    let blocks = match b.open.into_iter().next() {
        Some(Open::Page(blocks)) => blocks,
        _ => Vec::new(),
    };
    Page { title, blocks, links: b.links, headings: b.headings }
}

/// `link`, read from the page at `here`, as the path from the repository's
/// root it names: `../docs/API.md` from `examples/README.md` is
/// `docs/API.md`.
fn resolve(here: &str, link: &str) -> String {
    let mut parts: Vec<&str> = here.split('/').collect();
    parts.pop();
    for part in link.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

/// A table's column widths, no wider together than `room` when that can be
/// helped: each column its natural width if they fit, and otherwise the wide
/// ones cut down to a common width and the narrow ones left whole -- so a
/// column of short names stays on one line and the prose beside it wraps.
fn fit(natural: &[f32], room: f32) -> Vec<f32> {
    if natural.iter().sum::<f32>() <= room {
        return natural.to_vec();
    }
    let mut sorted = natural.to_vec();
    sorted.sort_by(f32::total_cmp);
    let mut left = room;
    let mut cap = 0.0;
    for (i, width) in sorted.iter().enumerate() {
        let share = left / (sorted.len() - i) as f32;
        if *width > share {
            cap = share;
            break;
        }
        left -= width;
    }
    natural.iter().map(|w| w.min(cap).max(MIN_COLUMN)).collect()
}

/// What a page is drawn in.
struct Look {
    text: Color32,
    /// Headings.
    strong: Color32,
    /// A quote's text.
    weak: Color32,
    link: Color32,
    /// Behind inline code.
    code: Color32,
    /// A code block's fill and a table head's.
    fill: Color32,
    /// Rules, borders, a quote's bar.
    line: Color32,
    /// Every other table row.
    stripe: Color32,
    palette: code::Palette,
    /// How far apart bold is drawn twice: egui has no bold face, and a
    /// second copy a physical pixel to the right thickens the strokes.
    bold: f32,
}

impl Look {
    fn of(ui: &egui::Ui, theme: UiTheme) -> Self {
        let v = ui.visuals();
        // VS Code's link colour, which its Catppuccin port makes blue.
        let (link, code, fill, line, stripe) = match theme {
            UiTheme::CatppuccinMocha => (
                palette::BLUE,
                palette::SURFACE0,
                palette::MANTLE,
                palette::SURFACE1,
                Color32::from_rgba_unmultiplied(0x31, 0x32, 0x44, 90),
            ),
            UiTheme::Dark => (
                Color32::from_rgb(0x37, 0x94, 0xff),
                Color32::from_gray(52),
                Color32::from_gray(22),
                Color32::from_gray(62),
                Color32::from_white_alpha(6),
            ),
        };
        Self {
            text: v.text_color(),
            strong: v.strong_text_color(),
            weak: v.weak_text_color(),
            link,
            code,
            fill,
            line,
            stripe,
            palette: code::palette(theme),
            bold: 1.0 / ui.ctx().pixels_per_point(),
        }
    }
}

/// What drawing a page carries through its blocks.
struct Draw<'a> {
    look: &'a Look,
    /// The page's path, which its pictures' paths are read from.
    here: &'a str,
    pictures: &'a HashMap<String, egui::TextureHandle>,
    links: &'a [String],
    /// A link clicked this frame.
    clicked: &'a mut Option<String>,
    /// The text's colour: dimmer in a quote.
    color: Color32,
    /// How deep in lists: the bullets' shapes, and the closer spacing.
    depth: usize,
}

/// What a run of a paragraph's characters is, beyond its text.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mark {
    Code,
    Link(usize),
}

/// Text laid out: the galley, the same again with only its bold showing,
/// and which characters are code or links.
struct Laid {
    galley: Arc<Galley>,
    bold: Option<Arc<Galley>>,
    marks: Vec<(Range<usize>, Mark)>,
}

/// `spans` laid out `wrap` wide at `size`, lines `spacing` apart, in
/// `color` -- all of it bold with `bold`, as a heading is.
#[allow(clippy::too_many_arguments)]
fn lay(ui: &egui::Ui, spans: &[Span], size: f32, spacing: f32, color: Color32, bold: bool, look: &Look, wrap: f32) -> Laid {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let line = (size * spacing).round();
    let mut marks = Vec::new();
    let mut strong = Vec::new();
    let mut at = 0;
    for span in spans {
        let code = span.style & CODE != 0;
        let font = if code { FontId::monospace((size * 0.88).round()) } else { FontId::proportional(size) };
        let color = if span.link.is_some() { look.link } else { color };
        let format = TextFormat {
            font_id: font,
            color,
            line_height: Some(line),
            italics: span.style & EMPH != 0,
            strikethrough: if span.style & STRIKE != 0 { Stroke::new(1.0, color) } else { Stroke::NONE },
            ..Default::default()
        };
        job.append(&span.text, 0.0, format);
        let n = span.text.chars().count();
        if code {
            marks.push((at..at + n, Mark::Code));
        }
        if let Some(link) = span.link {
            marks.push((at..at + n, Mark::Link(link)));
        }
        strong.push(bold || span.style & STRONG != 0);
        at += n;
    }
    // The same text with everything but the bold made invisible: laid out
    // the same, since only colours differ, so it lies exactly over it.
    let bold = strong.iter().any(|s| *s).then(|| {
        let mut job = job.clone();
        for (section, strong) in job.sections.iter_mut().zip(&strong) {
            if !strong {
                section.format.color = Color32::TRANSPARENT;
                section.format.strikethrough = Stroke::NONE;
            }
        }
        ui.painter().layout_job(job)
    });
    Laid { galley: ui.painter().layout_job(job), bold, marks }
}

/// Which part of a character a run of them is measured by.
#[derive(Clone, Copy)]
enum Part {
    /// The font's box: what inline code's background fills.
    Font,
    /// The whole row, line spacing and all: what a link answers the pointer
    /// over.
    Row,
    /// A pixel under the baseline: a link's underline.
    Under,
}

/// The rectangles characters `range` of `galley` cover, one per row they
/// run over, in the galley's coordinates.
fn runs(galley: &Galley, range: &Range<usize>, part: Part) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut index = 0;
    for row in &galley.rows {
        let mut run: Option<Rect> = None;
        for glyph in &row.glyphs {
            if range.contains(&index) {
                let x = row.pos.x + glyph.pos.x;
                let baseline = row.pos.y + glyph.pos.y;
                let (top, bottom) = match part {
                    Part::Font => (baseline - glyph.font_ascent, baseline - glyph.font_ascent + glyph.font_height),
                    Part::Row => (row.pos.y, row.pos.y + row.size.y),
                    Part::Under => (baseline + 1.0, baseline + 2.0),
                };
                let rect = Rect::from_min_max(pos2(x, top), pos2(x + glyph.advance_width, bottom));
                run = Some(run.map_or(rect, |r| r.union(rect)));
            }
            index += 1;
        }
        out.extend(run);
        // The `\n` a row ends with has no glyph, and is a character.
        if row.ends_with_newline {
            index += 1;
        }
    }
    out
}

/// Where the first line's baseline is, down from the galley's top.
fn first_baseline(galley: &Galley) -> f32 {
    galley.rows.first().map_or(0.0, |row| row.pos.y + row.glyphs.first().map_or(row.size.y * 0.8, |g| g.pos.y))
}

/// `laid` drawn at `at`: code on its background, bold thickened, the link
/// under the pointer underlined -- and, clicked, followed.
fn draw(ui: &egui::Ui, at: egui::Pos2, laid: &Laid, response: &egui::Response, d: &mut Draw) {
    let painter = ui.painter();
    let offset = at.to_vec2();
    for (range, mark) in &laid.marks {
        if *mark == Mark::Code {
            for run in runs(&laid.galley, range, Part::Font) {
                painter.rect_filled(run.translate(offset).expand2(vec2(2.0, 1.0)), 3.0, d.look.code);
            }
        }
    }
    painter.galley(at, laid.galley.clone(), d.color);
    if let Some(bold) = &laid.bold {
        painter.galley(at + vec2(d.look.bold, 0.0), bold.clone(), d.color);
    }
    // A click is placed where it pressed: a tap -- a touch screen's, or a
    // click the pointer left straight after -- ends with no pointer to hover.
    let clicked = response.clicked();
    let pointer = if clicked { response.interact_pointer_pos() } else { response.hover_pos() };
    let Some(pointer) = pointer else { return };
    for (range, mark) in &laid.marks {
        let Mark::Link(link) = *mark else { continue };
        if !runs(&laid.galley, range, Part::Row).iter().any(|r| r.translate(offset).contains(pointer)) {
            continue;
        }
        let to = &d.links[link];
        if response.hovered() {
            for run in runs(&laid.galley, range, Part::Under) {
                painter.rect_filled(run.translate(offset), 0.0, d.look.link);
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let _ = response.clone().on_hover_text_at_pointer(to.as_str());
        }
        if clicked {
            *d.clicked = Some(to.clone());
        }
        return;
    }
}

/// Text the width of the column, returning its first baseline.
fn text_block(ui: &mut egui::Ui, spans: &[Span], size: f32, spacing: f32, bold: bool, d: &mut Draw) -> f32 {
    let width = ui.available_width();
    let laid = lay(ui, spans, size, spacing, d.color, bold, d.look, width);
    let sense = if laid.marks.iter().any(|(_, m)| matches!(m, Mark::Link(_))) { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(width, laid.galley.size().y), sense);
    draw(ui, rect.min, &laid, &response, d);
    rect.top() + first_baseline(&laid.galley)
}

/// A heading, with room above it unless it opens the page, and under the
/// first two levels a rule, as GitHub draws them.
fn heading(ui: &mut egui::Ui, level: u8, spans: &[Span], first: bool, d: &mut Draw) -> f32 {
    let (size, above) = match level {
        1 => (26.0, 28.0),
        2 => (20.0, 26.0),
        3 => (16.5, 20.0),
        4 => (15.0, 16.0),
        _ => (BODY, 14.0),
    };
    if !first {
        ui.add_space(above);
    }
    let color = std::mem::replace(&mut d.color, d.look.strong);
    let baseline = text_block(ui, spans, size, 1.3, true, d);
    d.color = color;
    if level <= 2 {
        ui.add_space(6.0);
        let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
        ui.painter().hline(rule.x_range(), rule.center().y, Stroke::new(1.0, d.look.line));
    }
    ui.add_space(10.0);
    baseline
}

/// A code block: coloured as the editor colours code, on a card of its own,
/// and never wrapped -- Python wrapped is Python with its indentation gone --
/// so a long line scrolls sideways. Under the pointer, a copy button.
fn code_block(ui: &mut egui::Ui, lang: Lang, text: &str, d: &mut Draw) {
    let mut job = code::layout(text, lang, &d.look.palette, FontId::monospace(12.5));
    for section in &mut job.sections {
        section.format.line_height = Some(18.0);
    }
    let galley = ui.painter().layout_job(job);
    let pad = vec2(12.0, 9.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y + 2.0 * pad.y), Sense::hover());
    ui.painter().rect(rect, 6.0, d.look.fill, Stroke::new(1.0, d.look.line), egui::StrokeKind::Inside);
    // The text itself is the identity: a page is parsed once and kept.
    let id = egui::Id::new(("docs code", text.as_ptr() as usize));
    if galley.size().x + 2.0 * pad.x <= rect.width() {
        ui.painter().galley(rect.min + pad, galley, d.look.text);
    } else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(1.0)).id_salt(id));
        let color = d.look.text;
        egui::ScrollArea::horizontal().id_salt(id).show(&mut child, |ui| {
            let (area, _) = ui.allocate_exact_size(galley.size() + 2.0 * pad, Sense::hover());
            ui.painter().galley(area.min + pad, galley, color);
        });
    }
    if ui.rect_contains_pointer(rect) {
        let button = Rect::from_min_size(pos2(rect.right() - 30.0, rect.top() + 5.0), vec2(24.0, 22.0));
        let response = ui.interact(button, id.with("copy"), Sense::click());
        let now = ui.input(|i| i.time);
        let copied = ui.data(|m| m.get_temp::<f64>(id.with("copied"))).is_some_and(|t| now - t < 1.5);
        if response.hovered() {
            ui.painter().rect_filled(button, 4.0, d.look.code);
        }
        let icon = if copied { icons::codicon::CHECK } else { icons::codicon::COPY };
        let color = if response.hovered() { d.look.text } else { d.look.weak };
        ui.painter().text(button.center(), egui::Align2::CENTER_CENTER, icon, FontId::proportional(14.0), color);
        if response.clicked() {
            ui.ctx().copy_text(text.to_owned());
            ui.data_mut(|m| m.insert_temp(id.with("copied"), now));
        }
        response.on_hover_text(if copied { "Copied" } else { "Copy" });
    }
}

/// A picture's texture and size: `width` wide if the page says so, else its
/// own, never wider than `room`, its height to scale. `None` for one the tab
/// does not have.
fn sized(src: &str, width: Option<f32>, room: f32, d: &Draw) -> Option<(egui::TextureId, egui::Vec2)> {
    let texture = d.pictures.get(&resolve(d.here, src))?;
    let [w, h] = texture.size().map(|v| v as f32);
    let width = width.unwrap_or(w).min(room);
    Some((texture.id(), vec2(width, width * h / w.max(1.0))))
}

/// The pictures of every page, made textures, by their path.
fn textures(ctx: &egui::Context, pages: &[Page]) -> HashMap<String, egui::TextureHandle> {
    let mut out = HashMap::new();
    let mut found = Vec::new();
    for (source, page) in SOURCES.iter().zip(pages) {
        found.clear();
        images(&page.blocks, &mut found);
        for src in &found {
            let path = resolve(source.path, src);
            let Some(png) = picture(&path) else { continue };
            let Ok(image) = image::load_from_memory(png).map(|i| i.into_rgba8()) else { continue };
            let size = [image.width() as usize, image.height() as usize];
            let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            let texture = ctx.load_texture(&path, pixels, egui::TextureOptions::LINEAR);
            out.insert(path, texture);
        }
    }
    out
}

/// A list: each item's blocks indented, its bullet -- or number -- beside
/// its first line.
fn list(ui: &mut egui::Ui, start: Option<u64>, items: &[Vec<Block>], d: &mut Draw) {
    let left = ui.max_rect().left();
    for (n, item) in items.iter().enumerate() {
        let top = ui.cursor().top();
        let body = Rect::from_min_max(pos2(left + ITEM, top), ui.max_rect().right_bottom());
        d.depth += 1;
        let first = ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| blocks(ui, item, d)).inner;
        d.depth -= 1;
        let baseline = first.unwrap_or(top + BODY);
        let painter = ui.painter();
        match start {
            Some(start) => {
                let number = painter.layout_no_wrap(format!("{}.", start + n as u64), FontId::proportional(BODY), d.color);
                let at = pos2(left + ITEM - 6.0 - number.size().x, baseline - first_baseline(&number));
                painter.galley(at, number, d.color);
            }
            None => {
                let center = pos2(left + ITEM / 2.0 - 2.0, baseline - BODY * 0.3);
                match d.depth % 3 {
                    0 => painter.circle_filled(center, 2.5, d.color),
                    1 => painter.circle_stroke(center, 2.3, Stroke::new(1.1, d.color)),
                    _ => painter.rect_filled(Rect::from_center_size(center, vec2(4.4, 4.4)), 0.0, d.color),
                };
            }
        }
    }
}

/// A quote: its blocks dimmed and indented, a bar beside them.
fn quote(ui: &mut egui::Ui, inner: &[Block], d: &mut Draw) -> Option<f32> {
    let left = ui.max_rect().left();
    let top = ui.cursor().top();
    let body = Rect::from_min_max(pos2(left + 16.0, top), ui.max_rect().right_bottom());
    let color = std::mem::replace(&mut d.color, d.look.weak);
    let shown = ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| blocks(ui, inner, d));
    d.color = color;
    // Short of the gap the last block leaves under itself.
    let bottom = (shown.response.rect.bottom() - 8.0).max(top + 4.0);
    ui.painter().rect_filled(Rect::from_min_max(pos2(left + 2.0, top), pos2(left + 5.0, bottom)), 1.5, d.look.line);
    shown.inner
}

/// A table: columns as wide as their widest cell, cut down to fit where they
/// must and wrapped; the head on the code blocks' fill, every other row
/// striped, and lines between, as GitHub draws one.
fn table(ui: &mut egui::Ui, align: &[Alignment], head: &[Vec<Span>], rows: &[Vec<Vec<Span>>], d: &mut Draw) {
    const SIZE: f32 = 13.5;
    const SPACED: f32 = 1.4;
    let pad = vec2(10.0, 6.0);
    // Text sits at the top of its line, the spacing under it: half of that
    // moved above, so a cell's text is in its middle.
    let nudge = SIZE * (SPACED - 1.2) / 2.0;
    let columns = rows.iter().map(Vec::len).chain([head.len()]).max().unwrap_or(0);
    if columns == 0 {
        return;
    }
    let lines: Vec<&[Vec<Span>]> = std::iter::once(head).chain(rows.iter().map(Vec::as_slice)).collect();
    // A row short of cells has empty ones.
    fn cell(line: &[Vec<Span>], c: usize) -> &[Span] {
        line.get(c).map_or(&[][..], Vec::as_slice)
    }
    let mut natural = vec![0.0f32; columns];
    for (r, line) in lines.iter().enumerate() {
        for (c, width) in natural.iter_mut().enumerate() {
            let laid = lay(ui, cell(line, c), SIZE, SPACED, d.color, r == 0, d.look, f32::INFINITY);
            *width = width.max(laid.galley.size().x + 2.0 * pad.x + 1.0);
        }
    }
    let widths = fit(&natural, ui.available_width());
    let laid: Vec<Vec<Laid>> = lines
        .iter()
        .enumerate()
        .map(|(r, line)| {
            (0..columns)
                .map(|c| lay(ui, cell(line, c), SIZE, SPACED, d.color, r == 0, d.look, widths[c] - 2.0 * pad.x))
                .collect()
        })
        .collect();
    let heights: Vec<f32> = laid
        .iter()
        .map(|row| row.iter().map(|l| l.galley.size().y).fold(0.0, f32::max) + 2.0 * pad.y)
        .collect();
    let size = vec2(widths.iter().sum(), heights.iter().sum());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::click());
    let table = Rect::from_min_size(rect.min, size);
    let painter = ui.painter();
    let stroke = Stroke::new(1.0, d.look.line);
    let last = heights.len() - 1;
    let mut y = table.top();
    for (r, h) in heights.iter().enumerate() {
        let row = Rect::from_min_size(pos2(table.left(), y), vec2(size.x, *h));
        let bottom = if r == last { 6 } else { 0 };
        if r == 0 {
            painter.rect_filled(row, egui::CornerRadius { nw: 6, ne: 6, sw: bottom, se: bottom }, d.look.fill);
        } else if r % 2 == 0 {
            painter.rect_filled(row, egui::CornerRadius { nw: 0, ne: 0, sw: bottom, se: bottom }, d.look.stripe);
        }
        if r > 0 {
            painter.hline(row.x_range(), y, stroke);
        }
        y += h;
    }
    let mut x = table.left();
    for w in &widths[..columns - 1] {
        x += w;
        painter.vline(x, table.y_range(), stroke);
    }
    painter.rect_stroke(table, 6.0, stroke, egui::StrokeKind::Inside);
    let mut y = table.top();
    for (r, row) in laid.iter().enumerate() {
        let mut x = table.left();
        for (c, laid) in row.iter().enumerate() {
            let room = widths[c] - 2.0 * pad.x;
            let dx = match align.get(c) {
                Some(Alignment::Center) => (room - laid.galley.size().x) / 2.0,
                Some(Alignment::Right) => room - laid.galley.size().x,
                _ => 0.0,
            };
            draw(ui, pos2(x + pad.x + dx.max(0.0), y + pad.y + nudge), laid, &response, d);
            x += widths[c];
        }
        y += heights[r];
    }
}

/// `blocks`, one under another. Returns where the first line of text sits --
/// its baseline -- for a list item's bullet.
fn blocks(ui: &mut egui::Ui, blocks: &[Block], d: &mut Draw) -> Option<f32> {
    let mut first = None;
    for (i, block) in blocks.iter().enumerate() {
        let baseline = one(ui, block, i == 0, d);
        first = first.or(baseline);
    }
    first
}

/// One block and the room under it.
fn one(ui: &mut egui::Ui, block: &Block, first: bool, d: &mut Draw) -> Option<f32> {
    // A list's paragraphs sit closer: an item is a line, not a section.
    let gap = if d.depth > 0 { 4.0 } else { 10.0 };
    match block {
        Block::Heading { level, text } => Some(heading(ui, *level, text, first, d)),
        Block::Paragraph(spans) => {
            let baseline = text_block(ui, spans, BODY, SPACING, false, d);
            ui.add_space(gap);
            Some(baseline)
        }
        Block::Code { lang, text } => {
            code_block(ui, *lang, text, d);
            ui.add_space(gap + 4.0);
            None
        }
        Block::Rule => {
            ui.add_space(8.0);
            let (rule, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().hline(rule.x_range(), rule.center().y, Stroke::new(1.0, d.look.line));
            ui.add_space(16.0);
            None
        }
        Block::Quote(inner) => {
            let baseline = quote(ui, inner, d);
            ui.add_space(gap);
            baseline
        }
        Block::List { start, items } => {
            list(ui, *start, items, d);
            if d.depth == 0 {
                ui.add_space(gap);
            }
            None
        }
        Block::Table { align, head, rows } => {
            table(ui, align, head, rows, d);
            ui.add_space(gap + 6.0);
            None
        }
        // On a line of its own: floated right only at a page's top level,
        // where `page` lays the blocks beside it out narrower.
        Block::Image { src, width, .. } => {
            if let Some((id, size)) = sized(src, *width, ui.available_width(), d) {
                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                egui::Image::new(egui::load::SizedTexture::new(id, size)).paint_at(ui, rect);
                ui.add_space(gap);
            }
            None
        }
    }
}

/// An outline row: a heading, how deep, the section it is under, and
/// whether it has rows under it.
struct Row {
    heading: usize,
    depth: usize,
    parent: Option<usize>,
    children: bool,
}

/// The outline: the sections, `##`, and under each its `###`; a `###` with
/// no section above it at the top. The page's title, its first `#`, is the
/// page's own row already, and deeper headings are left to the page.
fn outline(headings: &[Heading]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut section: Option<usize> = None;
    let mut titled = false;
    for (i, h) in headings.iter().enumerate() {
        match h.level {
            1 if !titled => {
                titled = true;
                section = None;
            }
            1 | 2 => {
                section = Some(rows.len());
                rows.push(Row { heading: i, depth: 0, parent: None, children: false });
            }
            3 => match section {
                Some(s) => {
                    rows[s].children = true;
                    let parent = rows[s].heading;
                    rows.push(Row { heading: i, depth: 1, parent: Some(parent), children: false });
                }
                None => rows.push(Row { heading: i, depth: 0, parent: None, children: false }),
            },
            _ => {}
        }
    }
    rows
}

/// A title over a list in the tab's left column, as VS Code heads the
/// sections of its side bar.
fn section_title(ui: &mut egui::Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        FontId::proportional(11.0),
        ui.visuals().strong_text_color(),
    );
}

/// Where a page's blocks lie: each one's height at the width and scale it
/// was drawn at. What lets the blocks out of view be skipped, and a heading
/// be scrolled to before it has been drawn.
#[derive(Default)]
struct Heights {
    width: f32,
    ppp: f32,
    of: Vec<Option<f32>>,
}

/// What a click on a page asks of the rest of the window.
#[derive(Debug, PartialEq)]
pub enum Request {
    /// A script or a mesh, opened as a click in the files tab opens it.
    Open(PathBuf),
    /// A folder, shown open in the files tab: relative to the working
    /// directory, as the tree's paths are.
    Reveal(PathBuf),
}

/// The documentation tab.
#[derive(Default)]
pub struct Docs {
    /// The pages, parsed the first time the tab is shown.
    pages: Vec<Page>,
    heights: Vec<Heights>,
    /// Their pictures, by path.
    pictures: HashMap<String, egui::TextureHandle>,
    /// The page shown.
    current: usize,
    /// A block to bring to the top of its page, once the heights above it
    /// are known: a heading clicked in the outline, or a link's anchor.
    jump: Option<(usize, usize)>,
    /// The heading last scrolled past, which the outline lights.
    reading: Option<usize>,
    /// The outline's sections shown open, by page and heading.
    open: HashSet<(usize, usize)>,
    /// The outline row the outline was last scrolled to: it follows the
    /// page, and only when the page moves on to another heading.
    followed: Option<(usize, usize)>,
}

impl Docs {
    /// The tab: the pages and the outline of the one shown in a column on
    /// the left, the page on the right. Returns what a click asked of the
    /// rest of the window.
    pub fn show(&mut self, ui: &mut egui::Ui, theme: UiTheme) -> Option<Request> {
        if self.pages.is_empty() {
            self.pages = SOURCES.iter().map(|s| parse(s.text)).collect();
            self.heights = SOURCES.iter().map(|_| Heights::default()).collect();
            self.pictures = textures(ui.ctx(), &self.pages);
        }
        // The column's edge a line as faint as a card's outline, rather than
        // egui's separator, which the theme draws in a widget's border.
        let nav = egui::Panel::left("docs nav")
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 0, right: 8, top: 0, bottom: 0 }))
            .show_separator_line(false)
            .resizable(true)
            .default_size(230.0)
            .size_range(150.0..=420.0)
            .show(ui, |ui| self.nav(ui, theme));
        let edge = nav.response.rect;
        ui.painter().vline(edge.right(), edge.y_range(), Stroke::new(1.0, theme::outline(theme)));
        let mut clicked = None;
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.page(ui, theme, &mut clicked));
        clicked.and_then(|to| self.follow(ui.ctx(), &to))
    }

    /// The left column: the pages, as the files tab lists files, and the
    /// shown page's outline, as VS Code's outline view -- its sections
    /// folded, the one being read lit, and the list kept on it.
    fn nav(&mut self, ui: &mut egui::Ui, theme: UiTheme) {
        let (hover, selected, guide) = theme::list(theme);
        let text = ui.visuals().text_color();
        let weak = ui.visuals().weak_text_color();
        let body = egui::TextStyle::Body.resolve(ui.style());
        ui.spacing_mut().item_spacing.y = 0.0;
        section_title(ui, "PAGES");
        for (i, source) in SOURCES.iter().enumerate() {
            let (dir, name) = source.path.rsplit_once('/').unwrap_or(("", source.path));
            let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
            let painter = ui.painter_at(rect);
            if i == self.current {
                painter.rect_filled(rect, 0.0, selected);
            } else if response.hovered() {
                painter.rect_filled(rect, 0.0, hover);
            }
            let (x, y) = (rect.left() + 8.0, rect.center().y);
            egui::Image::new(icons::for_file(name))
                .paint_at(ui, Rect::from_center_size(pos2(x + 8.0, y), vec2(16.0, 16.0)));
            let named = painter.text(pos2(x + 22.0, y), egui::Align2::LEFT_CENTER, name, body.clone(), text);
            // Where it is, dimmed, as VS Code tells apart two files of one
            // name: the two READMEs.
            if !dir.is_empty() {
                painter.text(pos2(named.right() + 6.0, y), egui::Align2::LEFT_CENTER, dir, FontId::proportional(11.5), weak);
            }
            let hover_text = format!("{}\n{}", self.pages[i].title, source.path);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hover_text).clicked() && i != self.current {
                self.current = i;
                self.reading = None;
            }
        }

        ui.add_space(10.0);
        section_title(ui, "OUTLINE");
        let page = self.current;
        let headings = &self.pages[page].headings;
        let rows = outline(headings);
        let shown = |open: &HashSet<(usize, usize)>, row: &Row| row.parent.is_none_or(|p| open.contains(&(page, p)));
        // The row of the heading being read, or of its section while that
        // is folded.
        let lit = self.reading.and_then(|reading| {
            rows.iter().filter(|r| r.heading <= reading && shown(&self.open, r)).last().map(|r| r.heading)
        });
        if rows.is_empty() {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::hover());
            let at = pos2(rect.left() + 8.0, rect.center().y);
            ui.painter().text(at, egui::Align2::LEFT_CENTER, "no sections", body.clone(), weak);
        }
        let (open, jump, followed) = (&mut self.open, &mut self.jump, &mut self.followed);
        egui::ScrollArea::vertical().id_salt(("docs outline", page)).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &rows {
                if !shown(&*open, row) {
                    continue;
                }
                let heading = &headings[row.heading];
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                let painter = ui.painter_at(rect);
                if lit == Some(row.heading) {
                    painter.rect_filled(rect, 0.0, selected);
                } else if response.hovered() {
                    painter.rect_filled(rect, 0.0, hover);
                }
                for level in 0..row.depth {
                    let x = rect.left() + level as f32 * INDENT + TWISTIE / 2.0;
                    painter.vline(x, rect.y_range(), Stroke::new(1.0, guide));
                }
                let x = rect.left() + row.depth as f32 * INDENT;
                if row.children {
                    let chevron = if open.contains(&(page, row.heading)) {
                        icons::codicon::CHEVRON_DOWN
                    } else {
                        icons::codicon::CHEVRON_RIGHT
                    };
                    painter.text(
                        pos2(x + TWISTIE / 2.0, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        chevron,
                        FontId::proportional(14.0),
                        text,
                    );
                }
                let name_x = x + TWISTIE + 2.0;
                let mut job = LayoutJob::single_section(
                    heading.text.clone(),
                    TextFormat { font_id: body.clone(), color: text, ..Default::default() },
                );
                job.wrap = egui::text::TextWrapping::truncate_at_width((rect.right() - name_x - 4.0).max(0.0));
                let galley = painter.layout_job(job);
                painter.galley(pos2(name_x, rect.center().y - galley.size().y / 2.0), galley, text);
                if lit == Some(row.heading) && *followed != Some((page, row.heading)) {
                    response.scroll_to_me(None);
                    *followed = Some((page, row.heading));
                }
                let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&heading.text);
                if response.clicked() {
                    // The chevron folds; the rest of the row goes there, and
                    // opens a section on the way.
                    let on_chevron = response.interact_pointer_pos().is_some_and(|p| p.x < name_x);
                    if row.children && on_chevron {
                        if !open.remove(&(page, row.heading)) {
                            open.insert((page, row.heading));
                        }
                    } else {
                        *jump = Some((page, heading.block));
                        if row.children {
                            open.insert((page, row.heading));
                        }
                    }
                }
            }
        });
    }

    /// The page, in a column no wider than reads well, the blocks out of
    /// view skipped at their remembered heights.
    fn page(&mut self, ui: &mut egui::Ui, theme: UiTheme, clicked: &mut Option<String>) {
        let look = Look::of(ui, theme);
        let index = self.current;
        let page = &self.pages[index];
        let heights = &mut self.heights[index];
        let pictures = &self.pictures;
        let ppp = ui.ctx().pixels_per_point();
        let width = (ui.available_width() - 2.0 * MARGIN).clamp(160.0, WIDTH);
        if heights.width != width || heights.ppp != ppp || heights.of.len() != page.blocks.len() {
            *heights = Heights { width, ppp, of: vec![None; page.blocks.len()] };
        }
        // A scroll position per page, kept as egui keeps a scroll area's.
        let mut area = egui::ScrollArea::vertical().id_salt(("docs page", index)).auto_shrink([false, false]);
        if let Some((at, block)) = self.jump {
            let above = &heights.of[..block.min(heights.of.len())];
            if at == index && above.iter().all(Option::is_some) {
                // The first block is the page's top, margin and all; any
                // other brings the room above its heading to the top.
                let y = if block == 0 { 0.0 } else { MARGIN + above.iter().flatten().sum::<f32>() };
                area = area.vertical_scroll_offset(y);
                self.jump = None;
            }
        }
        let mut reading = None;
        area.show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().top();
            let left = ui.max_rect().left() + ((ui.max_rect().width() - width) / 2.0).max(0.0);
            let column = Rect::from_min_size(pos2(left, origin), vec2(width, ui.max_rect().height()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(column), |ui| {
                // Every gap is the renderer's own, so a block's height is
                // what it drew and nothing egui added between.
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                ui.add_space(MARGIN);
                let mut d = Draw {
                    look: &look,
                    here: SOURCES[index].path,
                    pictures,
                    links: &page.links,
                    clicked,
                    color: look.text,
                    depth: 0,
                };
                let mut next = 0;
                // A picture floated right: how far down it reaches, and the
                // room it takes from the blocks beside it.
                let mut float: Option<(f32, f32)> = None;
                for (i, block) in page.blocks.iter().enumerate() {
                    let top = ui.cursor().top() - origin;
                    if page.headings.get(next).is_some_and(|h| h.block == i) {
                        if top <= viewport.top() + 1.0 {
                            reading = Some(next);
                        }
                        next += 1;
                    }
                    // Floated as GitHub floats a README's logo: it takes no
                    // height of its own, and what follows is laid out
                    // narrower beside it until past its bottom. Placed in
                    // view or not, since the blocks beside it were measured
                    // narrowed and are skipped at those heights.
                    if let Block::Image { src, width, right: true } = block {
                        if let Some((id, size)) = sized(src, *width, ui.available_width() / 2.0, &d) {
                            let at = pos2(ui.max_rect().right() - size.x, ui.cursor().top());
                            let rect = Rect::from_min_size(at, size);
                            egui::Image::new(egui::load::SizedTexture::new(id, size)).paint_at(ui, rect);
                            float = Some((rect.bottom(), size.x + FLOAT_GAP));
                        }
                        heights.of[i] = Some(0.0);
                        continue;
                    }
                    let beside = float.filter(|(bottom, _)| ui.cursor().top() < *bottom).map(|(_, room)| room);
                    if let Some(h) = heights.of[i] {
                        if top + h < viewport.top() - 200.0 || top > viewport.bottom() + 200.0 {
                            ui.add_space(h);
                            continue;
                        }
                    }
                    // Nothing above it but the margin -- a float aside -- so
                    // a heading takes no room over itself.
                    let first = top <= MARGIN + 0.5;
                    let before = ui.cursor().top();
                    match beside {
                        Some(room) => {
                            let narrow = Rect::from_min_max(
                                ui.cursor().min,
                                pos2(ui.max_rect().right() - room, ui.max_rect().bottom()),
                            );
                            ui.scope_builder(egui::UiBuilder::new().max_rect(narrow), |ui| one(ui, block, first, &mut d));
                        }
                        None => {
                            one(ui, block, first, &mut d);
                        }
                    }
                    heights.of[i] = Some(ui.cursor().top() - before);
                }
                ui.add_space(MARGIN);
            });
        });
        self.reading = reading;
    }

    /// Follow a link clicked on the shown page.
    fn follow(&mut self, ctx: &egui::Context, to: &str) -> Option<Request> {
        if to.contains("://") || to.starts_with("mailto:") {
            ctx.open_url(egui::OpenUrl::new_tab(to));
            return None;
        }
        let (path, anchor) = to.split_once('#').unwrap_or((to, ""));
        let here = SOURCES[self.current].path;
        let target = if path.is_empty() { here.to_string() } else { resolve(here, path) };
        if let Some(page) = SOURCES.iter().position(|s| s.path == target) {
            self.current = page;
            self.reading = None;
            let block = self.pages[page].headings.iter().find(|h| h.anchor == anchor).map_or(0, |h| h.block);
            self.jump = Some((page, block));
            return None;
        }
        // A file of the repository: from this disk when it is on it -- run
        // from the repository or from a release bundle, which ships
        // `examples/` beside the executable -- and otherwise on GitHub.
        let relative: PathBuf = target.split('/').collect();
        let cwd = std::env::current_dir().ok();
        let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
        for base in cwd.iter().chain(beside.iter()) {
            let path = base.join(&relative);
            let opens = matches!(path.extension().and_then(|e| e.to_str()), Some("py" | "rs" | "obj"));
            let in_tree = cwd.as_ref() == Some(base);
            if path.is_file() && opens {
                return Some(Request::Open(if in_tree { relative } else { path }));
            }
            if path.is_dir() && in_tree {
                return Some(Request::Reveal(relative));
            }
        }
        ctx.open_url(egui::OpenUrl::new_tab(format!("{}/blob/main/{target}", env!("CARGO_PKG_REPOSITORY"))));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heading_is_anchored_as_github_anchors_it() {
        assert_eq!(slug("What a mesh carries"), "what-a-mesh-carries");
        assert_eq!(slug("app.config.focus: bool — default False (live)"), "appconfigfocus-bool--default-false-live");
        assert_eq!(slug("Mouse and trackpad — Arcball (the default)"), "mouse-and-trackpad--arcball-the-default");
        assert_eq!(slug("snake_case and CamelCase"), "snake_case-and-camelcase");
    }

    #[test]
    fn a_page_comes_out_as_written() {
        let page = parse(concat!(
            "# Title\r\n\r\n",
            "Some *text*, `code` and a [link](other.md#there).\r\n\r\n",
            "- one\r\n- two\r\n  - under\r\n\r\n",
            "| a | b |\r\n|---|:-:|\r\n| 1 | **2** |\r\n\r\n",
            "```python\r\nx = 1\r\n```\r\n\r\n",
            "> said\r\n\r\n---\r\n\r\n",
            "## Part\r\n\r\n### Detail\r\n\r\n## Part\r\n",
        ));
        assert_eq!(page.title, "Title");
        assert_eq!(page.links, ["other.md#there"]);
        let spans = |b: &Block| match b {
            Block::Paragraph(s) | Block::Heading { text: s, .. } => s.clone(),
            other => panic!("{other:?}"),
        };
        let p = spans(&page.blocks[1]);
        assert_eq!(p[1], Span { text: "text".into(), style: EMPH, link: None });
        assert_eq!(p[3], Span { text: "code".into(), style: CODE, link: None });
        assert_eq!(p[5], Span { text: "link".into(), style: 0, link: Some(0) });
        let Block::List { start: None, items } = &page.blocks[2] else { panic!("{:?}", page.blocks[2]) };
        assert_eq!(items.len(), 2);
        assert!(matches!(&items[1][..], [Block::Paragraph(_), Block::List { .. }]), "{:?}", items[1]);
        let Block::Table { align, head, rows } = &page.blocks[3] else { panic!("{:?}", page.blocks[3]) };
        assert_eq!(align, &[Alignment::None, Alignment::Center]);
        assert_eq!(head.len(), 2);
        assert_eq!(rows[0][1], [Span { text: "2".into(), style: STRONG, link: None }]);
        assert_eq!(page.blocks[4], Block::Code { lang: Lang::Python, text: "x = 1".into() });
        assert!(matches!(&page.blocks[5], Block::Quote(inner) if inner.len() == 1));
        assert_eq!(page.blocks[6], Block::Rule);
        let anchors: Vec<_> = page.headings.iter().map(|h| (h.level, h.anchor.as_str(), h.block)).collect();
        assert_eq!(anchors, [(1, "title", 0), (2, "part", 7), (3, "detail", 8), (2, "part-1", 9)]);
        let rows = outline(&page.headings);
        let shape: Vec<_> = rows.iter().map(|r| (r.heading, r.depth, r.parent, r.children)).collect();
        assert_eq!(shape, [(1, 0, None, true), (2, 1, Some(1), false), (3, 0, None, false)]);
    }

    #[test]
    fn an_img_tag_is_a_picture() {
        let page = parse("<img src=\"a/logo.png\" alt=\"\" width=\"120\" align=\"right\">\n\n# Title\n");
        assert_eq!(page.blocks[0], Block::Image { src: "a/logo.png".into(), width: Some(120.0), right: true });
        assert!(matches!(page.blocks[1], Block::Heading { level: 1, .. }));
        assert_eq!(page.headings[0].block, 1);
        let bare = parse("<img width=64 src=x.png>\n");
        assert_eq!(bare.blocks, [Block::Image { src: "x.png".into(), width: Some(64.0), right: false }]);
        // `src` inside another attribute's name is not `src`.
        assert_eq!(attr("<img data-src=\"no\" src=\"yes\">", "src").as_deref(), Some("yes"));
    }

    #[test]
    fn a_link_is_read_from_its_page() {
        assert_eq!(resolve("examples/README.md", "../docs/API.md"), "docs/API.md");
        assert_eq!(resolve("examples/README.md", "cube/light.py"), "examples/cube/light.py");
        assert_eq!(resolve("docs/API.md", "CONFIG.md"), "docs/CONFIG.md");
        assert_eq!(resolve("CHANGELOG.md", "./README.md"), "README.md");
    }

    #[test]
    fn a_table_fits_by_cutting_its_widest_columns() {
        assert_eq!(fit(&[100.0, 200.0], 400.0), [100.0, 200.0]);
        assert_eq!(fit(&[100.0, 600.0, 900.0], 1000.0), [100.0, 450.0, 450.0]);
        assert_eq!(fit(&[10.0, 900.0], 100.0), [64.0, 90.0]);
    }

    /// Every link in the pages leads somewhere: to a page, and a heading on
    /// it, or to a file of the repository. A page moved or a heading renamed
    /// shows up here rather than as a click that does nothing.
    #[test]
    fn every_link_leads_somewhere() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let pages: Vec<Page> = SOURCES.iter().map(|s| parse(s.text)).collect();
        let (mut broken, mut checked) = (Vec::new(), 0);
        for (source, page) in SOURCES.iter().zip(&pages) {
            for link in &page.links {
                if link.contains("://") || link.starts_with("mailto:") {
                    continue;
                }
                checked += 1;
                let (path, anchor) = link.split_once('#').unwrap_or((link, ""));
                let target = if path.is_empty() { source.path.to_string() } else { resolve(source.path, path) };
                let fine = match SOURCES.iter().position(|s| s.path == target) {
                    Some(p) => anchor.is_empty() || pages[p].headings.iter().any(|h| h.anchor == anchor),
                    None => anchor.is_empty() && root.join(&target).exists(),
                };
                if !fine {
                    broken.push(format!("{}: {link}", source.path));
                }
            }
            // And every picture is one the tab has compiled in.
            let mut found = Vec::new();
            images(&page.blocks, &mut found);
            for src in found {
                if picture(&resolve(source.path, src)).is_none() {
                    broken.push(format!("{}: picture {src}", source.path));
                }
            }
        }
        assert!(broken.is_empty(), "{broken:#?}");
        // Forty today, most of them the examples' README's scripts and
        // folders: a parser that found none would pass the loop above.
        assert!(checked >= 30, "only {checked} links read");
    }

    /// Every page drawn, at two widths: measured the first frame, the blocks
    /// out of view skipped after -- and a heading jumped to is the one read.
    #[test]
    fn every_page_draws_and_a_heading_can_be_reached() {
        let ctx = egui::Context::default();
        super::super::icons::install(&ctx);
        let mut docs = Docs::default();
        let frame = |docs: &mut Docs, width: f32| {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 700.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                assert_eq!(docs.show(ui, UiTheme::CatppuccinMocha), None);
            });
            output.textures_delta.clear();
        };
        for page in 0..SOURCES.len() {
            for width in [1200.0, 520.0] {
                docs.current = page;
                frame(&mut docs, width);
                frame(&mut docs, width);
                let heights = &docs.heights[page];
                assert!(heights.of.iter().all(Option::is_some), "{}: every block measured", SOURCES[page].path);
                assert!(heights.of.iter().all(|h| h.is_some_and(|h| h >= 0.0)), "{}", SOURCES[page].path);
            }
        }
        // The config's "Shadows", far down its page.
        let config = SOURCES.iter().position(|s| s.path == "docs/CONFIG.md").unwrap();
        docs.current = config;
        let shadows = docs.pages[config].headings.iter().position(|h| h.anchor == "shadows").expect("a Shadows section");
        docs.jump = Some((config, docs.pages[config].headings[shadows].block));
        frame(&mut docs, 1200.0);
        frame(&mut docs, 1200.0);
        assert_eq!(docs.jump, None);
        assert_eq!(docs.reading, Some(shadows));
    }
}
