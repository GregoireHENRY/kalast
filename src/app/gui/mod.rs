//! The editor shell: a Blender-style layout with the renderer as one panel.
//!
//! Deliberately *not* an overlay on the render window. `App::start()` and
//! `App::step()` keep drawing the scene straight to the swapchain, which is
//! what a script run from a terminal wants and what every existing example
//! gets. The editor is a second entry point, `App::start_editor()`, and the
//! only thing it changes about the renderer is where the scene lands: into
//! `render_texture` sized to the viewport panel, which egui then samples.
//!
//! That texture already carries `TEXTURE_BINDING` -- the scene has always
//! been drawn offscreen and blitted at the end -- so showing it in a panel
//! costs a sampler, not a copy.

mod code;
mod config_panel;
mod docs;
mod icon_table;
mod icons;
pub mod script;
mod simulation_panel;
mod theme;
mod widgets;

use std::collections::VecDeque;

/// kalast's logo, 256 points square: the window's and the taskbar's icon,
/// the empty scene's watermark, and the picture at the top of the README in
/// the documentation tab. One copy for the three.
pub(crate) static LOGO: &[u8] = include_bytes!("../../../res/kalast-256.png");

/// Lines shown in the log panel.
///
/// Bounded rather than growing: a run that prints per frame would otherwise
/// hold every line it ever wrote for as long as the window is open.
pub struct Log {
    entries: VecDeque<Entry>,
    limit: usize,
    /// Lines pushed since the panel last showed this log: its tab has a dot
    /// while there are any.
    unread: usize,
}

/// One line of the log, and when it was written: `17:42:10.123`, the local
/// clock to the millisecond.
pub struct Entry {
    pub time: String,
    pub text: String,
}

/// The log panel's two tabs: what kalast prints about itself, shown first,
/// and what the script prints, so the first never lands in the middle of the
/// second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LogTab {
    #[default]
    Kalast,
    Script,
    /// A Python console: lines run between frames, among the script's
    /// variables.
    Python,
}

/// What the middle of the window shows: the scene, the script, or kalast's
/// documentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CentralTab {
    #[default]
    Renderer,
    Editor,
    Docs,
}

/// The side panel's tabs: the app's own settings, the simulation's, and the
/// folder the app was started in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SideTab {
    App,
    #[default]
    Simulation,
    Files,
}

/// The scripts tab: two folders as a tree -- `examples`, the bundle's, which
/// an update replaces, and `scripts`, the user's own, which it never touches
/// -- listed as they are opened. A right click on a row offers what can be
/// done to it: a new file or folder, a rename, a move to the Trash; for a
/// file, the documentation tab or the renderer.
#[derive(Default)]
struct FileTree {
    /// Each opened folder's entries -- folders first, then files, by name --
    /// and when they were read. Re-read when older than `FRESH`, so a file a
    /// script writes turns up without the folder being read every frame.
    listings: std::collections::HashMap<std::path::PathBuf, (std::time::Instant, Vec<(String, bool)>)>,
    /// The folders shown open.
    open: std::collections::HashSet<std::path::PathBuf>,
    /// A name being typed into the tree.
    naming: Option<Naming>,
}

/// What a row of the tree asked for, served after the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TreeAction {
    /// A file clicked: into the editor.
    Open(std::path::PathBuf),
    /// Markdown's "Open as documentation".
    Docs(std::path::PathBuf),
    /// A script's or a mesh's "Send to renderer".
    Render(std::path::PathBuf),
    /// "Delete": asked about, then to the Trash.
    Trash(std::path::PathBuf),
    /// A file "New file" made: into the editor.
    Created(std::path::PathBuf),
    /// A file renamed: the editor follows it.
    Renamed(std::path::PathBuf, std::path::PathBuf),
}

/// A name being typed into the tree, as VS Code's explorer takes one: in a
/// row of its own, Enter to make it, Escape to give up.
struct Naming {
    what: Named,
    text: String,
    /// The field asks for the focus once, as it appears.
    focus: bool,
    /// Why the last Enter was refused.
    error: Option<String>,
}

enum Named {
    /// A new folder, in this one.
    Folder(std::path::PathBuf),
    /// A new file, in this one.
    File(std::path::PathBuf),
    /// This file's new name.
    Rename(std::path::PathBuf),
}

/// Where `examples` and `scripts` are: beside the executable in a release
/// bundle, wherever it was started from, and otherwise in the working
/// directory -- the repository's, a project's.
fn scripts_base() -> std::path::PathBuf {
    crate::app::bundled_python_dir()
        .and_then(|python| python.parent().map(tree_path))
        .unwrap_or_default()
}

/// `path` relative to the working directory when inside it, as the tree
/// names what it lists; whole otherwise.
fn tree_path(path: &std::path::Path) -> std::path::PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
}

/// The editor's files: the one shown -- its text, its path and whether it is
/// edited, the `Editor`'s own fields, which the rest of the window reads --
/// and the others opened this run, kept aside with their edits. A view onto
/// those fields, so that opening, switching, closing and renaming can be
/// tested without a window.
struct Buffers<'a> {
    script: &'a mut String,
    path: &'a mut String,
    dirty: &'a mut bool,
    opened: &'a mut Vec<String>,
    stash: &'a mut std::collections::HashMap<String, (String, bool)>,
}

impl Buffers<'_> {
    /// Show `path`: switched to if it is open, edits and all, else read --
    /// text only, and small enough to edit -- and added to the list. The one
    /// it replaces is kept aside.
    fn open(&mut self, path: &std::path::Path) -> Result<(), String> {
        let key = path.display().to_string();
        if key == self.path.trim() {
            return Ok(());
        }
        let (text, dirty) = match self.stash.remove(&key) {
            Some(buffer) => buffer,
            None => (read_text(path)?, false),
        };
        self.stash_shown();
        *self.script = text;
        *self.path = key.clone();
        *self.dirty = dirty;
        if !self.opened.contains(&key) {
            self.opened.push(key);
        }
        Ok(())
    }

    /// The one shown, kept aside -- an untitled one too, when there is
    /// something in it.
    fn stash_shown(&mut self) {
        let key = self.path.trim().to_string();
        if key.is_empty() && self.script.trim().is_empty() {
            return;
        }
        self.stash.insert(key.clone(), (std::mem::take(self.script), *self.dirty));
        if !self.opened.contains(&key) {
            self.opened.push(key);
        }
    }

    /// Close `path`, its edits already asked about: the file after it in the
    /// list shown in its place when it was the one shown, else nothing.
    fn close(&mut self, path: &str) {
        let at = self.opened.iter().position(|p| p == path);
        self.opened.retain(|p| p != path);
        self.stash.remove(path);
        if path != self.path.trim() {
            return;
        }
        let next = at.and_then(|i| self.opened.get(i.min(self.opened.len().saturating_sub(1))).cloned());
        match next.and_then(|n| self.stash.remove(&n).map(|buffer| (n, buffer))) {
            Some((n, (text, dirty))) => {
                *self.script = text;
                *self.path = n;
                *self.dirty = dirty;
            }
            None => {
                self.script.clear();
                self.path.clear();
                *self.dirty = false;
            }
        }
    }

    /// A file or a folder renamed on the disk: the buffers of what was in
    /// it follow it.
    fn renamed(&mut self, from: &str, to: &str) {
        if let Some(moved) = moved(self.path.trim(), from, to) {
            *self.path = moved;
        }
        for p in self.opened.iter_mut() {
            if let Some(moved) = moved(p, from, to) {
                *p = moved;
            }
        }
        let keys: Vec<String> = self.stash.keys().filter(|k| moved(k, from, to).is_some()).cloned().collect();
        for key in keys {
            if let (Some(buffer), Some(moved)) = (self.stash.remove(&key), moved(&key, from, to)) {
                self.stash.insert(moved, buffer);
            }
        }
    }
}

/// Where `path` is once `from` -- it, or a folder above it -- is renamed
/// `to`; `None` when it is not under `from`.
fn moved(path: &str, from: &str, to: &str) -> Option<String> {
    if from.is_empty() || path.is_empty() {
        return None;
    }
    let rest = std::path::Path::new(path).strip_prefix(from).ok()?;
    Some(if rest.as_os_str().is_empty() { to.to_string() } else { std::path::Path::new(to).join(rest).display().to_string() })
}

/// A file's text, for the editor: refused when it is not text -- a NUL in
/// its first 8 KiB, or not UTF-8 -- or too big to edit at a frame's pace.
fn read_text(path: &std::path::Path) -> Result<String, String> {
    const MAX: u64 = 4 << 20;
    let size = std::fs::metadata(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?.len();
    if size > MAX {
        return Err(format!("{} is {} MB, too big to edit here", path.display(), size / 1_000_000));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(format!("{} is not text", path.display()));
    }
    String::from_utf8(bytes).map_err(|_| format!("{} is not text: not UTF-8", path.display()))
}

/// A file the renderer takes: a script to run, a mesh to show.
fn renderable(path: &std::path::Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()), Some("py" | "rs" | "obj"))
}

/// A tree row's height, a level's indent and the chevron's column, in
/// points: VS Code's explorer.
const ROW: f32 = 22.0;
const INDENT: f32 = 10.0;
const TWISTIE: f32 = 16.0;

impl FileTree {
    const FRESH: std::time::Duration = std::time::Duration::from_secs(2);

    /// `dir`'s entries, hidden ones and `__pycache__` left out: none for a
    /// folder that is not there yet, as `scripts` may not be.
    fn list(&mut self, dir: &std::path::Path) -> Vec<(String, bool)> {
        if let Some((at, entries)) = self.listings.get(dir) {
            if at.elapsed() < Self::FRESH {
                return entries.clone();
            }
        }
        let read = if dir.as_os_str().is_empty() { std::path::Path::new(".") } else { dir };
        let mut entries: Vec<(String, bool)> = std::fs::read_dir(read)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let hidden = name.starts_with('.') || name == "__pycache__";
                (!hidden).then(|| (name, e.file_type().is_ok_and(|t| t.is_dir())))
            })
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        self.listings.insert(dir.to_path_buf(), (std::time::Instant::now(), entries.clone()));
        entries
    }

    /// Everything shown read again at the next frame: after a file is made,
    /// renamed or thrown away.
    fn reread(&mut self) {
        self.listings.clear();
    }

    /// Draw the two folders as a tree, returning what a row asked for.
    ///
    /// Drawn as VS Code's explorer draws it rather than with egui's
    /// collapsing headers: rows the width of the panel, lit under the pointer
    /// and for the open file, a chevron and Catppuccin's icon on each, levels
    /// joined by indent guides. A folder opens on a click anywhere on its row.
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        current: &std::path::Path,
        theme: crate::app::config::UiTheme,
    ) -> Option<TreeAction> {
        ui.spacing_mut().item_spacing.y = 0.0;
        let base = scripts_base();
        let mut action = None;
        for root in ["examples", "scripts"] {
            self.row(ui, &base.join(root), root, true, 0, current, theme, &mut action);
        }
        action
    }

    /// `dir`'s rows: a name being typed for a new entry first, then each
    /// entry, the one being renamed as its field.
    fn children(
        &mut self,
        ui: &mut egui::Ui,
        dir: &std::path::Path,
        depth: usize,
        current: &std::path::Path,
        theme: crate::app::config::UiTheme,
        action: &mut Option<TreeAction>,
    ) {
        let new_here = match self.naming.as_ref().map(|n| &n.what) {
            Some(Named::Folder(d)) => (d == dir).then_some(true),
            Some(Named::File(d)) => (d == dir).then_some(false),
            _ => None,
        };
        if let Some(is_dir) = new_here {
            self.naming_row(ui, depth, is_dir, "", action);
        }
        for (name, is_dir) in self.list(dir) {
            let path = dir.join(&name);
            if matches!(self.naming.as_ref().map(|n| &n.what), Some(Named::Rename(p)) if *p == path) {
                self.naming_row(ui, depth, is_dir, &name, action);
                continue;
            }
            self.row(ui, &path, &name, is_dir, depth, current, theme, action);
        }
    }

    /// One entry's row, its menu, and its children when it is an open
    /// folder.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        path: &std::path::Path,
        name: &str,
        is_dir: bool,
        depth: usize,
        current: &std::path::Path,
        theme: crate::app::config::UiTheme,
        action: &mut Option<TreeAction>,
    ) {
        let (hover, selected, guide) = theme::list(theme);
        let open = is_dir && self.open.contains(path);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::click());
        let painter = ui.painter_at(rect);
        if !is_dir && path == current {
            painter.rect_filled(rect, 0.0, selected);
        } else if response.hovered() || response.context_menu_opened() {
            painter.rect_filled(rect, 0.0, hover);
        }
        // One guide per level above this one, through the middle of the
        // chevron column of the folder it belongs to.
        for level in 0..depth {
            let x = rect.left() + level as f32 * INDENT + TWISTIE / 2.0;
            painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, guide));
        }

        let mut x = rect.left() + depth as f32 * INDENT;
        let text_color = ui.visuals().text_color();
        if is_dir {
            let chevron = if open { icons::codicon::CHEVRON_DOWN } else { icons::codicon::CHEVRON_RIGHT };
            painter.text(
                egui::pos2(x + TWISTIE / 2.0, rect.center().y),
                egui::Align2::CENTER_CENTER,
                chevron,
                egui::FontId::proportional(14.0),
                text_color,
            );
        }
        x += TWISTIE + 2.0;
        let icon = if is_dir { icons::for_folder(name, open) } else { icons::for_file(name) };
        egui::Image::new(icon).paint_at(
            ui,
            egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), egui::vec2(16.0, 16.0)),
        );
        x += 16.0 + 6.0;
        painter.text(
            egui::pos2(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::TextStyle::Body.resolve(ui.style()),
            text_color,
        );

        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            if is_dir {
                if !self.open.remove(path) {
                    self.open.insert(path.to_path_buf());
                }
            } else {
                *action = Some(TreeAction::Open(path.to_path_buf()));
            }
        }
        response.context_menu(|ui| self.menu(ui, path, is_dir, depth == 0, action));
        if open {
            self.children(ui, path, depth + 1, current, theme, action);
        }
    }

    /// A row's menu: `examples` takes a new example, `scripts` a new folder
    /// or file, a folder in either a new file, a new name or the Trash, and a
    /// file a new name or the Trash -- and, as it is, the documentation tab or
    /// the renderer.
    fn menu(&mut self, ui: &mut egui::Ui, path: &std::path::Path, is_dir: bool, root: bool, action: &mut Option<TreeAction>) {
        // Lit under the pointer, as a menu's rows are; no frame otherwise.
        let item = |ui: &mut egui::Ui, icon: &str, text: &str| {
            ui.add(egui::Button::new(format!("{icon}  {text}")).frame_when_inactive(false)).clicked()
        };
        use icons::codicon as c;
        let dir = path.to_path_buf();
        if is_dir {
            let examples = path.file_name().is_some_and(|n| n == "examples");
            if root && examples {
                if item(ui, c::NEW_FOLDER, "New example\u{2026}") {
                    self.start(Named::Folder(dir));
                }
                return;
            }
            if root {
                if item(ui, c::NEW_FOLDER, "New folder\u{2026}") {
                    self.start(Named::Folder(dir.clone()));
                }
                if item(ui, c::NEW_FILE, "New file\u{2026}") {
                    self.start(Named::File(dir));
                }
                return;
            }
            if item(ui, c::NEW_FILE, "New file\u{2026}") {
                self.start(Named::File(dir.clone()));
            }
            if item(ui, c::EDIT, "Rename\u{2026}") {
                self.start(Named::Rename(dir.clone()));
            }
            ui.separator();
            if item(ui, c::TRASH, "Delete") {
                *action = Some(TreeAction::Trash(dir));
            }
            return;
        }
        if docs::is_markdown(path) && item(ui, c::OPEN_PREVIEW, "Open as documentation") {
            *action = Some(TreeAction::Docs(dir.clone()));
        }
        if renderable(path) && item(ui, c::SEND, "Send to renderer") {
            *action = Some(TreeAction::Render(dir.clone()));
        }
        if item(ui, c::EDIT, "Rename\u{2026}") {
            self.start(Named::Rename(dir.clone()));
        }
        ui.separator();
        if item(ui, c::TRASH, "Delete") {
            *action = Some(TreeAction::Trash(dir));
        }
    }

    /// A name to type, for `what`: its folder opened so the field shows.
    fn start(&mut self, what: Named) {
        let text = match &what {
            Named::Rename(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            Named::Folder(d) | Named::File(d) => {
                self.open.insert(d.clone());
                String::new()
            }
        };
        self.naming = Some(Naming { what, text, focus: true, error: None });
    }

    /// The row a name is typed in: its icon, and the field.
    fn naming_row(&mut self, ui: &mut egui::Ui, depth: usize, is_dir: bool, was: &str, action: &mut Option<TreeAction>) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
        let x = rect.left() + depth as f32 * INDENT + TWISTIE + 2.0;
        let named = self.naming.as_ref().map(|n| n.text.clone()).unwrap_or_default();
        let shown = if named.is_empty() { was.to_string() } else { named };
        let icon = if is_dir { icons::for_folder(&shown, false) } else { icons::for_file(&shown) };
        egui::Image::new(icon).paint_at(
            ui,
            egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), egui::vec2(16.0, 16.0)),
        );
        let field = egui::Rect::from_min_max(egui::pos2(x + 22.0, rect.top() + 1.0), egui::pos2(rect.right() - 4.0, rect.bottom() - 1.0));
        let Some(n) = self.naming.as_mut() else { return };
        let response = ui.put(
            field,
            egui::TextEdit::singleline(&mut n.text).margin(egui::vec2(4.0, 1.0)).id(egui::Id::new("tree naming")),
        );
        if std::mem::take(&mut n.focus) {
            response.request_focus();
        }
        if let Some(e) = &n.error {
            ui.painter().rect_stroke(field, 2.0, egui::Stroke::new(1.0, ui.visuals().error_fg_color), egui::StrokeKind::Outside);
            response.clone().on_hover_text(e.clone());
        }
        if response.lost_focus() {
            let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if escape {
                self.naming = None;
            } else {
                self.commit(enter, action);
            }
        }
    }

    /// Make what is being named: a folder, an empty file, a new name. A name
    /// that will not do is said and the field kept -- after Enter; a click
    /// away gives it up, as it gives up an empty one.
    fn commit(&mut self, enter: bool, action: &mut Option<TreeAction>) {
        let Some(n) = self.naming.as_mut() else { return };
        let name = n.text.trim().to_string();
        let target = match &n.what {
            Named::Folder(d) | Named::File(d) => d.join(&name),
            Named::Rename(p) => p.with_file_name(&name),
        };
        let unchanged = matches!(&n.what, Named::Rename(p) if *p == target);
        let refused = if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            Some(format!("`{name}` is no name for a file"))
        } else if !unchanged && target.exists() {
            Some(format!("{name} is there already"))
        } else {
            None
        };
        if unchanged || (refused.is_some() && (!enter || name.is_empty())) {
            self.naming = None;
            return;
        }
        if let Some(why) = refused {
            n.error = Some(why);
            n.focus = true;
            return;
        }
        let made = match &n.what {
            Named::Folder(d) | Named::File(d) => std::fs::create_dir_all(d).and_then(|()| match &n.what {
                Named::Folder(_) => std::fs::create_dir(&target).map(|()| None),
                _ => std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map(|_| Some(TreeAction::Created(target.clone()))),
            }),
            Named::Rename(p) => std::fs::rename(p, &target).map(|()| Some(TreeAction::Renamed(p.clone(), target.clone()))),
        };
        match made {
            Ok(done) => {
                match &n.what {
                    Named::Folder(_) => {
                        self.open.insert(target);
                    }
                    // A folder renamed: it, and those in it, stay open.
                    Named::Rename(from) => {
                        let open: Vec<std::path::PathBuf> = self.open.iter().filter(|p| p.starts_with(from)).cloned().collect();
                        for p in open {
                            self.open.remove(&p);
                            if let Ok(rest) = p.strip_prefix(from) {
                                self.open.insert(target.join(rest));
                            }
                        }
                    }
                    Named::File(_) => {}
                }
                self.naming = None;
                self.reread();
                if done.is_some() {
                    *action = done;
                }
            }
            Err(e) => {
                n.error = Some(e.to_string());
                n.focus = true;
            }
        }
    }
}

/// What a click in the editor's column asked for.
enum NavClick {
    /// A file open, to show.
    Show(String),
    /// Its cross.
    Close(String),
    /// A symbol of the outline: its line.
    Line(usize),
}

/// The editor's column, as the documentation tab has its pages and theirs:
/// the files open -- the one shown lit, an edited one dotted, the one the
/// renderer runs marked, each with a cross under the pointer -- and the
/// shown one's outline, VS Code's, a click going to the line.
#[allow(clippy::too_many_arguments)]
fn editor_nav(
    ui: &mut egui::Ui,
    t: crate::app::config::UiTheme,
    opened: &[String],
    shown: &str,
    edited: &std::collections::HashSet<String>,
    rendering: Option<&str>,
    symbols: &[script::outline::Symbol],
) -> Option<NavClick> {
    use script::outline::Kind;
    let (hover, selected, guide) = theme::list(t);
    let text = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let body = egui::TextStyle::Body.resolve(ui.style());
    let mut click = None;
    ui.spacing_mut().item_spacing.y = 0.0;
    docs::section_title(ui, "OPEN FILES");
    let mut files: Vec<&str> = opened.iter().map(String::as_str).collect();
    if !files.contains(&shown) && !shown.is_empty() {
        files.push(shown);
    }
    if files.is_empty() {
        widgets::note(ui, "none: a click in the scripts tab opens one");
    }
    for path in files {
        let (dir, name) = path.rsplit_once(['/', '\\']).unwrap_or(("", path));
        let name = if name.is_empty() { "untitled" } else { name };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::click());
        let painter = ui.painter_at(rect);
        if path == shown {
            painter.rect_filled(rect, 0.0, selected);
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, hover);
        }
        let (x, y) = (rect.left() + 8.0, rect.center().y);
        egui::Image::new(icons::for_file(name)).paint_at(ui, egui::Rect::from_center_size(egui::pos2(x + 8.0, y), egui::vec2(16.0, 16.0)));
        let named = painter.text(egui::pos2(x + 22.0, y), egui::Align2::LEFT_CENTER, name, body.clone(), text);
        if !dir.is_empty() {
            painter.text(egui::pos2(named.right() + 6.0, y), egui::Align2::LEFT_CENTER, dir, egui::FontId::proportional(11.5), weak);
        }
        // On the right: the cross under the pointer, else the edited dot;
        // before it, the renderer's mark.
        let cross = egui::Rect::from_center_size(egui::pos2(rect.right() - 12.0, y), egui::vec2(18.0, 18.0));
        if response.hovered() {
            let over = response.hover_pos().is_some_and(|p| cross.contains(p));
            if over {
                painter.rect_filled(cross, 3.0, hover.gamma_multiply(2.0));
            }
            painter.text(cross.center(), egui::Align2::CENTER_CENTER, icons::codicon::CLOSE, egui::FontId::proportional(14.0), text);
        } else if edited.contains(path) {
            painter.text(cross.center(), egui::Align2::CENTER_CENTER, icons::codicon::CIRCLE_FILLED, egui::FontId::proportional(10.0), text);
        }
        if rendering == Some(path) {
            painter.text(
                egui::pos2(cross.left() - 10.0, y),
                egui::Align2::CENTER_CENTER,
                icons::codicon::SEND,
                egui::FontId::proportional(13.0),
                theme::accent(t),
            );
        }
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if rendering == Some(path) {
            format!("{path}\nsent to the renderer: Play and Restart run it")
        } else {
            path.to_string()
        });
        if response.clicked() {
            let on_cross = response.interact_pointer_pos().is_some_and(|p| cross.contains(p));
            click = Some(if on_cross { NavClick::Close(path.to_string()) } else { NavClick::Show(path.to_string()) });
        }
    }

    ui.add_space(10.0);
    docs::section_title(ui, "OUTLINE");
    if symbols.is_empty() {
        widgets::note(ui, if shown.is_empty() { "no file shown" } else { "nothing to outline in this file" });
    }
    egui::ScrollArea::vertical().id_salt("editor outline").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for symbol in symbols {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::click());
            let painter = ui.painter_at(rect);
            if response.hovered() {
                painter.rect_filled(rect, 0.0, hover);
            }
            for level in 0..symbol.depth {
                let x = rect.left() + level as f32 * INDENT + TWISTIE / 2.0;
                painter.vline(x, rect.y_range(), egui::Stroke::new(1.0, guide));
            }
            // VS Code's glyph and colour for the kind.
            let (glyph, color) = match symbol.kind {
                Kind::Class => (icons::codicon::SYMBOL_CLASS, theme::palette::PEACH),
                Kind::Function | Kind::Method => (icons::codicon::SYMBOL_METHOD, theme::palette::MAUVE),
                Kind::Variable => (icons::codicon::SYMBOL_VARIABLE, theme::palette::BLUE),
                Kind::Field => (icons::codicon::SYMBOL_FIELD, theme::palette::BLUE),
                Kind::Constant => (icons::codicon::SYMBOL_CONSTANT, theme::palette::PEACH),
                Kind::Struct => (icons::codicon::SYMBOL_STRUCTURE, theme::palette::YELLOW),
                Kind::Enum => (icons::codicon::SYMBOL_ENUM, theme::palette::PEACH),
                Kind::Trait => (icons::codicon::SYMBOL_INTERFACE, theme::palette::BLUE),
                Kind::Module => (icons::codicon::SYMBOL_NAMESPACE, text),
                Kind::Implementation => (icons::codicon::SYMBOL_CLASS, theme::palette::YELLOW),
                Kind::Heading => (icons::codicon::SYMBOL_STRING, text),
            };
            let x = rect.left() + symbol.depth as f32 * INDENT + 8.0;
            painter.text(egui::pos2(x + 8.0, rect.center().y), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(14.0), color);
            painter.text(egui::pos2(x + 22.0, rect.center().y), egui::Align2::LEFT_CENTER, &symbol.name, body.clone(), text);
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                click = Some(NavClick::Line(symbol.line));
            }
        }
    });
    click
}

/// Where the UI app's window opens every time, shown while
/// `remember_window` is off -- filled, as the box is unticked, from the
/// window as it then stands. Written by hand rather than generated: the
/// screen is a list of those connected, where a name typed in would move the
/// window at each letter.
fn window_start(ui: &mut egui::Ui, a: &mut crate::app::config::AppConfig, screens: &[String]) {
    widgets::setting(ui, "screen", "The screen the window opens on.", |ui| {
        let shown = if a.monitor.is_empty() { "the main one".to_string() } else { a.monitor.clone() };
        egui::ComboBox::from_id_salt("a.monitor").selected_text(shown).show_ui(ui, |ui| {
            ui.selectable_value(&mut a.monitor, String::new(), "the main one");
            for name in screens {
                ui.selectable_value(&mut a.monitor, name.clone(), name);
            }
        });
    });
    let at = "Its top-left corner from the screen's, in pixels; -1 in the middle.";
    widgets::setting(ui, "window x", at, |ui| ui.add(egui::DragValue::new(&mut a.window_x).range(-1..=i32::MAX)));
    widgets::setting(ui, "window y", at, |ui| ui.add(egui::DragValue::new(&mut a.window_y).range(-1..=i32::MAX)));
    let size = "In pixels; 0 for most of the screen.";
    widgets::setting(ui, "window width", size, |ui| ui.add(egui::DragValue::new(&mut a.width)));
    widgets::setting(ui, "window height", size, |ui| ui.add(egui::DragValue::new(&mut a.height)));
    widgets::setting(ui, "open fullscreen", "Open fullscreen, whatever F did last time.", |ui| {
        ui.checkbox(&mut a.start_fullscreen, "")
    });
}

impl Log {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit,
            unread: 0,
        }
    }

    /// A line written now.
    pub fn push(&mut self, line: impl Into<String>) {
        self.push_at(crate::app::clock::now().time(), line);
    }

    /// A line written at `time`: when it reached the capture, which is
    /// earlier than the frame that moves it here.
    pub fn push_at(&mut self, time: String, line: impl Into<String>) {
        if self.entries.len() == self.limit {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry { time, text: line.into() });
        self.unread = self.unread.saturating_add(1);
    }

    /// A line that is no news, shown without a dot on its tab: the python
    /// tab's banner.
    pub fn push_seen_at(&mut self, time: String, line: impl Into<String>) {
        let unread = self.unread;
        self.push_at(time, line);
        self.unread = unread;
    }

    /// Whether lines have come in since `mark_read`.
    pub fn has_unread(&self) -> bool {
        self.unread > 0
    }

    /// The panel has shown everything in this log.
    pub fn mark_read(&mut self) {
        self.unread = 0;
    }

    /// The lines' text.
    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|e| &e.text)
    }

    /// The lines with their stamps, for the panel.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.unread = 0;
    }
}

#[cfg(test)]
mod log_tests {
    use super::Log;

    /// The console's state is one per process, so its tests take turns: two
    /// at once read each other's output, and took each other's Tab answers.
    static CONSOLE_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn console_turn() -> std::sync::MutexGuard<'static, ()> {
        CONSOLE_TESTS.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The python tab is greeted once, whoever asks first: the binary as it
    /// starts and Python after it both do. And quietly: the banner put a dot
    /// on the tab at every start, for two lines nobody had written.
    #[test]
    fn the_python_tab_is_greeted_once_and_quietly() {
        let _turn = console_turn();
        // Whatever an earlier test left, so the dot below is this one's.
        super::drain_console_output(&mut Log::new(8));
        super::console_greet("3.14.7 (main)", "win32");
        super::console_greet("3.14.7 (main)", "win32");
        let mut log = Log::new(8);
        super::drain_console_output(&mut log);
        let lines: Vec<&String> = log.lines().collect();
        assert_eq!(lines.iter().filter(|l| l.starts_with("Python 3.14.7 (main) on win32")).count(), 1, "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Type \"help\"")), "{lines:?}");
        assert!(!log.has_unread(), "no dot for the banner");

        super::console_write("42\n");
        super::drain_console_output(&mut log);
        assert!(log.has_unread(), "what a line prints is news");
    }

    /// Tab's answer goes into the line: one completion whole, several as
    /// what they have in common, listed above it by their last names.
    #[test]
    fn a_tab_answer_completes_the_line() {
        let _turn = console_turn();
        let ctx = egui::Context::default();
        let (mut input, mut history, mut back) = ("x = m.".to_string(), Vec::new(), 0);
        super::console_offer("x = m.".into(), "x = ".into(), vec!["m.mat".into(), "m.mesh".into()]);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(raw, |ui| super::console_prompt(ui, &mut input, &mut history, &mut back, &super::code::palette(crate::app::config::UiTheme::CatppuccinMocha)));
        output.textures_delta.clear();
        assert_eq!(input, "x = m.m", "what they have in common");
        let mut log = Log::new(8);
        super::drain_console_output(&mut log);
        assert_eq!(log.lines().last().map(String::as_str), Some("mat  mesh"), "listed");

        // An answer to a line since changed is dropped.
        super::console_offer("x = m.".into(), "x = ".into(), vec!["m.mat".into()]);
        let raw = egui::RawInput::default();
        let mut output = ctx.run_ui(raw, |ui| super::console_prompt(ui, &mut input, &mut history, &mut back, &super::code::palette(crate::app::config::UiTheme::CatppuccinMocha)));
        output.textures_delta.clear();
        assert_eq!(input, "x = m.m", "a stale answer changed the line");
    }

    /// `17:42:10.123`.
    pub(super) fn is_time(stamp: &str) -> bool {
        let b = stamp.as_bytes();
        b.len() == 12
            && b[2] == b':'
            && b[5] == b':'
            && b[8] == b'.'
            && [0, 1, 3, 4, 6, 7, 9, 10, 11].iter().all(|&i| b[i].is_ascii_digit())
    }

    /// Lines are unread from when they come in until the panel shows them,
    /// which is what puts the dot on a tab; clearing a log leaves nothing to
    /// read either.
    #[test]
    fn lines_are_unread_until_shown() {
        let mut log = Log::new(2);
        assert!(!log.has_unread(), "a new log has nothing to read");
        log.push("one");
        log.push("two");
        log.push("three");
        assert!(log.has_unread(), "lines came in");
        log.mark_read();
        assert!(!log.has_unread(), "the panel showed them");
        log.push("four");
        log.clear();
        assert!(!log.has_unread(), "a cleared log has nothing to read");
    }

    /// The log panel on the python tab keeps its height from frame to
    /// frame. The prompt's row was started at 18 points and its field was
    /// taller: centred, it stuck out past the panel, egui kept the panel that
    /// much taller, and it grew by a point every frame to its maximum.
    #[test]
    fn the_console_does_not_grow_its_panel() {
        let _turn = console_turn();
        let ctx = egui::Context::default();
        let (mut input, mut history, mut back) = (String::new(), Vec::new(), 0);
        let mut heights = Vec::new();
        for _ in 0..6 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(raw, |ui| {
                // The log's own card: outline, margins, rounding.
                let card = egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, egui::Color32::GRAY))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(8))
                    .outer_margin(egui::Margin::same(super::CARD_MARGIN));
                let panel = egui::Panel::bottom("log")
                    .frame(card)
                    .show_separator_line(false)
                    .resizable(true)
                    .default_size(160.0)
                    .show(ui, |ui| {
                    // As the log lays it out: the prompt after the lines,
                    // inside the scroll area.
                    let palette = super::code::palette(crate::app::config::UiTheme::CatppuccinMocha);
                    egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label("42");
                            super::console_prompt(ui, &mut input, &mut history, &mut back, &palette);
                        });
                    });
                });
                heights.push(panel.response.rect.height());
            });
            // Nothing draws them here; egui checks they were seen to.
            output.textures_delta.clear();
        }
        assert!(heights.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01), "{heights:?}");
    }

    /// Lines typed at the console come out in the order typed, empty ones
    /// included -- one closes a block -- and what running them printed
    /// reaches the python tab a whole line at a time.
    #[test]
    fn the_console_keeps_order_and_whole_lines() {
        let _turn = console_turn();
        super::console_submit("for i in range(2):".into());
        super::console_submit("".into());
        assert_eq!(super::console_take().as_deref(), Some("for i in range(2):"));
        assert_eq!(super::console_take().as_deref(), Some(""));
        assert_eq!(super::console_take(), None);

        super::console_write(">>> 6 * ");
        super::console_write("7\n42\npartial");
        let mut log = Log::new(8);
        super::drain_console_output(&mut log);
        assert_eq!(log.lines().collect::<Vec<_>>(), [">>> 6 * 7", "42"]);
        super::console_write("\n");
        super::drain_console_output(&mut log);
        assert_eq!(log.lines().last().map(String::as_str), Some("partial"));
    }

    /// A line pushed now carries the time; one pushed with a time of its own
    /// keeps it; and once the log is full the oldest goes.
    #[test]
    fn lines_carry_their_time() {
        let mut log = Log::new(2);
        log.push_at("07:04:03.250".to_string(), "caught");
        log.push("one");
        assert_eq!(log.entries().next().unwrap().time, "07:04:03.250");
        log.push("two");
        assert_eq!(log.lines().collect::<Vec<_>>(), ["one", "two"]);
        assert!(log.entries().all(|e| is_time(&e.time)), "stamped hh:mm:ss.mmm");
    }
}

/// How close to an edge the pointer must come to summon a panel, in points.
const EDGE: f32 = 24.0;

/// A card's outer margin: half the gap between two, VS Code's 4 points.
const CARD_MARGIN: i8 = 2;

/// How long the pointer rests on a panel's edge before it is lit: VS Code's
/// `workbench.sash.hoverDelay`.
const SASH_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// The panels there are, by their index in the four-slot arrays: toolbar,
/// log, side panel. Slot 2, the left edge, has held nothing since the script
/// became the middle's editor tab; it is kept so `panels_shown` and the
/// arrays keep their shape.
const DOCKED: [usize; 3] = [0, 1, 3];

/// How big each floating panel is when it has not been dragged: top, bottom,
/// left, right.
///
/// The top entry is the toolbar's and is not read: one row of buttons takes
/// that row's height, and nothing about it is dragged. It keeps its place so
/// the four panels index alike.
const FLOAT_DEFAULTS: [f32; 4] = [30.0, 160.0, 300.0, 240.0];

/// A *floating* panel smaller than this counts as put away rather than merely
/// narrow, and comes back at its usual size when next summoned.
///
/// The docked panels use a comfortable `min_size` instead -- 120 or 180 --
/// because there the same number is also the smallest a panel can be dragged,
/// and squeezing one through widths nothing can be read at is worse than
/// shutting it in one drag.
const COLLAPSE: f32 = 8.0;

/// Size of a toolbar action, in points: VS Code's, an icon in a square a
/// little wider than it is tall.
///
/// One size for every icon, so that none of them moves when one of them
/// changes: Play becomes Pause every time the simulation is held, and at the
/// rate `K` can be held down labels of two widths turned the whole row -- and
/// the iteration readout beside it -- into a blur.
const ACTION: egui::Vec2 = egui::vec2(28.0, 24.0);

/// Points the toolbar's logo is drawn at: the height of a tab's text and
/// then some, within the row's 24.
const BADGE: f32 = 18.0;

/// The logo at `BADGE` points on a screen of `ppp` pixels per point, scaled
/// down here rather than by the GPU: egui's wgpu textures have no mipmaps, so
/// the 256-pixel logo sampled down to 18 points came out jagged. Filtered on
/// premultiplied pixels, or the transparent corners' black would bleed into
/// the edge.
fn small_logo(ctx: &egui::Context, ppp: f32) -> Option<egui::TextureHandle> {
    let side = (BADGE * ppp).round().max(1.0) as u32;
    let mut rgba = image::load_from_memory(LOGO).ok()?.into_rgba8();
    for p in rgba.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 127) / 255) as u8;
        }
    }
    let mut small = image::imageops::resize(&rgba, side, side, image::imageops::FilterType::Lanczos3);
    // Lanczos rings a little past the alpha it sits in; premultiplied, no
    // colour can be brighter than its alpha.
    for p in small.pixels_mut() {
        for c in 0..3 {
            p[c] = p[c].min(p[3]);
        }
    }
    let pixels = egui::ColorImage::from_rgba_premultiplied([side as usize, side as usize], small.as_raw());
    Some(ctx.load_texture("kalast badge", pixels, egui::TextureOptions::LINEAR))
}

/// What a welcome line points at: keys, drawn as caps, or a tab of the side
/// panel, by its icon and name -- not a cap, which read as a key: "files tab"
/// looked like the `Tab` key.
enum Hint {
    Keys(&'static [&'static str]),
    Tab(&'static str, &'static str),
}

/// Nothing in the scene: the logo, faded, and what to do next under it -- as
/// VS Code's empty editor shows its own. `alpha` fades it out.
fn watermark(ui: &egui::Ui, rect: egui::Rect, logo: egui::TextureId, alpha: f32) {
    let side = (rect.width().min(rect.height()) * 0.26).clamp(72.0, 190.0);
    let hints: [(&str, Hint); 4] = [
        ("Open a script or a mesh", Hint::Tab(icons::codicon::FILES, "files")),
        ("Play or pause", Hint::Keys(&["P"])),
        ("Fold the panels", Hint::Keys(&["N"])),
        ("Give the window to the scene", Hint::Keys(&["Shift", "F"])),
    ];
    let row = 24.0;
    let block = side + 28.0 + hints.len() as f32 * row;
    // Too small a viewport to hold it is left alone.
    if rect.height() < block + 16.0 || rect.width() < 320.0 {
        return;
    }
    let top = rect.center().y - block / 2.0;
    let painter = ui.painter_at(rect);
    painter.image(
        logo,
        egui::Rect::from_center_size(egui::pos2(rect.center().x, top + side / 2.0), egui::vec2(side, side)),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::from_white_alpha((70.0 * alpha) as u8),
    );
    let weak = ui.visuals().weak_text_color().gamma_multiply(alpha);
    let font = egui::FontId::proportional(13.0);
    let key_font = egui::FontId::proportional(11.5);
    for (i, (what, hint)) in hints.iter().enumerate() {
        let y = top + side + 28.0 + i as f32 * row + row / 2.0;
        painter.text(egui::pos2(rect.center().x - 10.0, y), egui::Align2::RIGHT_CENTER, *what, font.clone(), weak);
        let mut x = rect.center().x + 10.0;
        match hint {
            // Each key a cap, as VS Code draws them.
            Hint::Keys(keys) => {
                for key in keys.iter() {
                    let galley = painter.layout_no_wrap(key.to_string(), key_font.clone(), weak);
                    let cap = egui::Rect::from_min_size(egui::pos2(x, y - 9.0), egui::vec2(galley.size().x + 10.0, 18.0));
                    painter.rect_stroke(cap, 3.0, egui::Stroke::new(1.0, weak.gamma_multiply(0.5)), egui::StrokeKind::Inside);
                    painter.galley(cap.center() - galley.size() / 2.0, galley, weak);
                    x = cap.right() + 4.0;
                }
            }
            // The tab as the side panel shows it: its icon, then its name.
            Hint::Tab(icon, name) => {
                let icon = painter.text(egui::pos2(x, y), egui::Align2::LEFT_CENTER, *icon, egui::FontId::proportional(15.0), weak);
                painter.text(egui::pos2(icon.right() + 5.0, y), egui::Align2::LEFT_CENTER, *name, font.clone(), weak);
            }
        }
    }
}

/// Where the camera is and how it sees, to tell whether it has moved: its
/// frame, and the projection's shape.
///
/// The `up` the view is drawn with, made perpendicular to it as the view
/// matrix makes it, rather than `up` as stored: a new app's is world up
/// until the first frame makes it perpendicular, and that is no move --
/// taken for one, it dismissed the welcome as the app opened.
fn camera_state(eye: &crate::app::frame::Eye) -> [crate::Float; 15] {
    let p = &eye.projection;
    let ortho = if p.mode == crate::app::frame::ProjectionMode::Orthographic { 1.0 } else { 0.0 };
    let dir = eye.dir.normalize_or_zero();
    let up = (eye.up - dir * eye.up.dot(dir)).normalize_or_zero();
    [
        eye.pos.x,
        eye.pos.y,
        eye.pos.z,
        dir.x,
        dir.y,
        dir.z,
        up.x,
        up.y,
        up.z,
        eye.anchor.x,
        eye.anchor.y,
        eye.anchor.z,
        p.fovy,
        p.side.unwrap_or(-1.0),
        ortho,
    ]
}

/// A VS Code toolbar action: a Codicon alone, in its colour, with a
/// background only under the pointer. What it does goes in its hover text.
fn action(icon: &str, color: egui::Color32) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(icon).size(16.0).color(color))
        .frame_when_inactive(false)
        .min_size(ACTION)
}

/// A VS Code tab: its label dim until it is chosen or under the pointer,
/// and the accent under the chosen one. `upper` for a panel's tabs, which
/// VS Code writes in capitals -- TERMINAL, OUTPUT.
fn panel_tab(ui: &mut egui::Ui, selected: bool, label: &str, upper: bool, accent: egui::Color32) -> egui::Response {
    let (text, size) = if upper { (label.to_uppercase(), 11.0) } else { (label.to_owned(), 13.0) };
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(size), egui::Color32::PLACEHOLDER);
    let padding = egui::vec2(6.0, 5.0);
    let (rect, response) = ui.allocate_exact_size(galley.size() + 2.0 * padding, egui::Sense::click());
    let color = if selected || response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.painter().galley(rect.min + padding, galley, color);
    if selected {
        ui.painter().hline(
            rect.x_range().shrink(padding.x - 2.0),
            rect.bottom() - 1.0,
            egui::Stroke::new(2.0, accent),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// VS Code's primary button, the vibrant one -- "Update", "Restart to
/// update": the accent filled in, an icon and a label on it in the darkest
/// shade.
fn primary(theme: crate::app::config::UiTheme, icon: &str, label: &str) -> egui::Button<'static> {
    let (fill, text) = theme::primary(theme);
    egui::Button::new(egui::RichText::new(format!("{icon}  {label}")).color(text).strong())
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(4))
}

/// Which panels to draw: `[top, bottom, left, right]`.
///
/// All of them unless the renderer has the window to itself, in which case
/// each is summoned by the pointer reaching its edge -- and stays while the
/// pointer is anywhere over it, since the 24-point strip is far narrower than
/// the panel and reaching for anything in one would otherwise dismiss it.
///
/// `panels` is where each was last drawn, or `Rect::NOTHING` for one that was
/// not; a panel that is not showing cannot keep itself showing.
///
/// A pure function so the arithmetic can be tested: driving a real pointer at
/// a real window is not possible from a test, because macOS delivers
/// mouse-moved events only to the front application.
fn reveal_panels(
    immersive: bool,
    pointer: Option<egui::Pos2>,
    screen: egui::Rect,
    panels: &[egui::Rect; 4],
) -> [bool; 4] {
    if !immersive {
        return [true; 4];
    }
    let Some(p) = pointer else {
        // The UI has never seen a pointer -- an unfocused window, usually.
        // Nothing to summon a panel with, so nothing is shown; the first move
        // inside the window fixes it.
        return [false; 4];
    };
    let near = [
        p.y <= screen.top() + EDGE,
        p.y >= screen.bottom() - EDGE,
        p.x <= screen.left() + EDGE,
        p.x >= screen.right() - EDGE,
    ];
    std::array::from_fn(|i| near[i] || panels[i].contains(p))
}

pub struct Editor {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,

    /// egui's handle on `render_texture`. Re-registered whenever that texture
    /// is reallocated, which a viewport resize does.
    viewport_texture: Option<egui::TextureId>,
    /// The size `render_texture` was registered at.
    registered_size: (u32, u32),
    /// And which incarnation of it, since a rebuild makes a new texture at
    /// the same size -- see `Window::render_generation`.
    registered_generation: u64,

    /// What the viewport panel measured last frame, in physical pixels.
    ///
    /// The scene has to be rendered *before* egui runs, so its size can only
    /// come from the previous frame's layout. On a resize the image is one
    /// frame stale, which is invisible; the alternative is a blank first
    /// frame at every new size.
    pub viewport_size: (u32, u32),
    /// Whether each docked panel is open: top, bottom, left, right.
    ///
    /// egui flips these itself -- dragging a resize handle past the panel's
    /// minimum shuts it, and the thin handle it leaves behind at the edge
    /// drags it back, as does a double click. The state has to live somewhere
    /// that outlasts a frame, which is here.
    ///
    /// The toolbar too. It is one row and cannot be made taller, but its
    /// lower edge folds it like the others: dragged up, or double-clicked.
    docked_open: [bool; 4],
    /// What `AppConfig::panels_folded` said last frame. The config's word for
    /// "all four shut" is applied to the panels when it changes, and written
    /// back from them every frame, so a script, the checkbox, `N` and a drag
    /// all agree.
    last_folded: bool,
    /// What the four per-panel fields -- `toolbar_folded`, `log_folded`,
    /// `script_folded`, `simulation_folded` -- said last frame, in
    /// `docked_open`'s order, for the same reason.
    last_each: [bool; 4],

    /// How big each floating panel is: top, bottom, left, right.
    ///
    /// Kept here because a floating panel is an `Area`, which has none of a
    /// docked panel's resize machinery -- so it is dragged by a strip drawn
    /// on its inner edge and the size remembered between reveals.
    float_sizes: [f32; 4],
    /// Which floating panel is being dragged, if any.
    ///
    /// It has to stay revealed while it is: a drag wanders off the panel
    /// almost immediately, and losing the panel mid-drag would make it
    /// impossible to make one bigger.
    resizing: Option<usize>,

    /// Where each panel sits, in egui points, or `NOTHING` when it is not
    /// shown.
    ///
    /// Kept so a revealed panel stays revealed while the pointer is on it:
    /// the edge strip that summons the right panel is 24 points wide and the
    /// panel is 240, so "near the edge" stops being true the moment you
    /// reach for anything in it.
    panels: [egui::Rect; 4],

    /// Where the viewport panel sits, in egui points.
    ///
    /// The scene is an egui `Image`, so egui reports the pointer as its own
    /// whenever it is over one -- and the camera controller, which is given
    /// what egui does not want, never saw a drag on the scene. This says
    /// where "the scene" is so those events can be let through.
    pub viewport_rect: egui::Rect,

    /// Which of the log's two tabs is showing.
    pub log_tab: LogTab,

    /// The theme the panels were last drawn in, so it is applied when
    /// `app.config.theme` changes rather than every frame.
    theme: Option<crate::app::config::UiTheme>,

    /// The python tab's input line; what was typed at it before, for `↑`;
    /// and how far back `↑` has gone, 0 being the line being typed.
    console_input: String,
    console_history: Vec<String>,
    console_back: usize,

    /// Which of the middle's tabs is showing: the scene, the script, or the
    /// documentation.
    pub central_tab: CentralTab,
    /// Which of the side panel's three tabs is showing.
    side_tab: SideTab,
    /// The panel edge the pointer is resting on, and since when: lit once it
    /// has rested there `SASH_DELAY`, as VS Code's are.
    sash_hover: Option<(usize, std::time::Instant)>,
    /// The panel edge being dragged, lit until the button comes up.
    sash_drag: Option<usize>,
    /// The files tab's listings.
    files: FileTree,
    /// The files opened this run, in the order they were: the one shown is
    /// `script_path`, its text in `script`; the others' text, and whether it
    /// is edited, are in `stash`, so that switching back loses nothing.
    opened: Vec<String>,
    stash: std::collections::HashMap<String, (String, bool)>,
    /// The script the renderer runs -- sent to it by the editor's render
    /// button or the scripts tab -- which Play, Restart and Step act on,
    /// whatever file the editor shows.
    pub rendering: Option<String>,
    /// A mesh sent to the renderer, for the app to show.
    pub mesh_request: Option<std::path::PathBuf>,
    /// Render asks for the scene as a new app has it first -- settings and
    /// camera too, which Restart keeps -- so that rendering again is a clean
    /// start. Taken by the app with the run it goes with.
    pub reset_before_render: bool,
    /// A path the scripts tab asked to throw away, waiting on the prompt.
    trash_asked: Option<std::path::PathBuf>,
    /// An edited file asked to be closed, waiting on the prompt.
    close_asked: Option<String>,
    /// What the editor has to say, for the log's kalast tab.
    notes: Vec<String>,

    /// The script buffer, so a simulation can be edited without leaving the
    /// window. Plain text, not a file handle: what is on screen is what
    /// `Run` executes, saved or not.
    pub script: String,
    pub script_path: String,
    /// Set when the buffer differs from what was last read or written.
    pub script_dirty: bool,
    /// The script editor's language servers, Neovim and popups.
    script_editor: script::ScriptEditor,
    /// The documentation tab: its pages, parsed once, and where each is at.
    docs: docs::Docs,
    /// The logo, for the empty scene's watermark. Made the first time it
    /// is shown.
    logo: Option<egui::TextureHandle>,
    /// The logo at the toolbar's start, made small for the screen it is on,
    /// and the pixels per point it was made for. See `small_logo`.
    badge: Option<(f32, egui::TextureHandle)>,
    /// The welcome goes the first time the camera moves, as Neovim's intro
    /// goes at the first key: the camera as it stood when the welcome came,
    /// whether it has gone, and frames to wait before taking the camera's
    /// state -- a reset lands between frames, and its new camera is not a
    /// move.
    welcome_camera: Option<[crate::Float; 15]>,
    welcome_gone: bool,
    welcome_settle: u8,
    /// The user reached for the camera this frame -- a drag, the wheel, a
    /// movement key -- whether or not it could move: set by the app from the
    /// controller, and what the welcome goes on. A new app's camera sits on
    /// its anchor, where a drag has nothing to orbit, so a welcome waiting
    /// for the camera to move waited for ever.
    pub camera_asked: bool,
    /// Raised by the buttons, drained by the app, which owns the Python
    /// side. The UI cannot run anything itself -- it has no interpreter and
    /// no business holding the GIL mid-layout.
    pub run_request: bool,
    /// Set when the run was asked for by Restart rather than Play. The scene
    /// is rebuilt either way; this says whether it then runs.
    pub restart_request: bool,
    pub open_request: bool,
    /// The toolbar's update and relaunch buttons; see `app::update`.
    pub update_request: bool,
    pub relaunch_request: bool,
    pub save_request: bool,
    /// The window was asked to close over an edited script. The editor puts
    /// the question; the answer comes back as one of the two below, or as
    /// nothing, which is Cancel.
    pub confirm_exit: bool,
    /// Quit without saving. Drained by the app, which owns the exit.
    pub quit_request: bool,
    /// Save first, then quit -- and if the save fails, stay.
    pub exit_after_save: bool,

    /// Which profile the Rust buttons act on. Release by default, because a
    /// debug build of this renderer is 2-15x slower and an example run for
    /// its numbers wants the fast one.
    pub rust_release: bool,
    pub build_request: bool,
    pub launch_request: bool,
    /// Play pressed on an example that was not built: the build is running
    /// and this says to launch it when it finishes.
    /// Whether a compiled binary exists for the `.rs` in the panel, at the
    /// selected profile. Refreshed by the app when the path, the profile or
    /// a build changes -- not every frame, since answering it means reading
    /// `Cargo.toml` and stat-ing a file.
    pub rust_built: bool,
    pub rust_key: (String, bool, u64),
    pub was_building: bool,
    /// A build the editor started itself, after a load found the library
    /// stale. Load again when it finishes.
    pub load_after_build: bool,
    /// Held while a `cargo build` thread is running, so the buttons can go
    /// grey rather than starting a second one on top of the first.
    pub building: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Editor {
    /// The script was written to disk: Neovim's buffer is no longer
    /// modified, and the language server checks it.
    pub fn script_saved(&mut self) {
        self.script_editor.saved();
    }

    /// Show `path` in the editor: switched to if it is open already, else
    /// read -- text only, and small enough to edit -- and added to the files
    /// opened. The buffer it replaces is kept, edits and all. Nothing reaches
    /// the renderer: that is the render button's.
    pub fn open_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.buffers().open(path)
    }

    /// The editor's files, as `Buffers` works on them.
    fn buffers(&mut self) -> Buffers<'_> {
        Buffers {
            script: &mut self.script,
            path: &mut self.script_path,
            dirty: &mut self.script_dirty,
            opened: &mut self.opened,
            stash: &mut self.stash,
        }
    }

    /// A script given whole -- the command line's, `app.set_script` --
    /// shown, with the one shown kept, and sent to the renderer: it is what
    /// runs.
    pub fn show_script(&mut self, path: String, source: String) {
        if path.trim() != self.script_path.trim() {
            self.stash.remove(path.trim());
            self.stash_current();
        }
        self.script = source;
        self.script_path = path.clone();
        self.script_dirty = false;
        if !self.opened.contains(&path) {
            self.opened.push(path.clone());
        }
        self.rendering = Some(path);
    }

    /// An untitled script given a file by "Save as": listed by it now.
    pub fn saved_as(&mut self, path: &str) {
        self.buffers().renamed("", path);
        self.script_path = path.to_string();
        if !self.opened.iter().any(|p| p == path) {
            self.opened.push(path.to_string());
        }
    }

    /// The file shown read again from the disk, its edits given up.
    pub fn reload_shown(&mut self) -> Result<(), String> {
        let path = std::path::PathBuf::from(self.script_path.trim());
        self.script = read_text(&path)?;
        self.script_dirty = false;
        let key = path.display().to_string();
        if !self.opened.contains(&key) {
            self.opened.push(key);
        }
        Ok(())
    }

    /// `open_file`, the middle turned to the editor -- or, a file that
    /// cannot be edited, said why.
    fn open_in_editor(&mut self, path: &std::path::Path) {
        match self.open_file(path) {
            Ok(()) => self.central_tab = CentralTab::Editor,
            Err(e) => self.notes.push(e),
        }
    }

    /// The buffer shown, kept aside for switching back to.
    fn stash_current(&mut self) {
        self.buffers().stash_shown();
    }

    /// Close `path`, its edits already asked about.
    fn close_file(&mut self, path: &str) {
        self.buffers().close(path);
    }

    /// Whether a file open has edits not saved: the one shown or another.
    pub fn any_dirty(&self) -> bool {
        self.script_dirty || self.stash.values().any(|(_, dirty)| *dirty)
    }

    /// The files with edits not saved, by path.
    fn dirty_names(&self) -> Vec<String> {
        let name = |p: &str| if p.is_empty() { "untitled".to_string() } else { p.to_string() };
        let mut names: Vec<String> = self.stash.iter().filter(|(_, (_, d))| *d).map(|(p, _)| name(p)).collect();
        if self.script_dirty {
            names.insert(0, name(self.script_path.trim()));
        }
        names
    }

    /// Write every edited file but the one shown -- which goes the usual
    /// way, through `save_request` -- as "Save all and quit" asks.
    fn save_stashed(&mut self) {
        for (path, (text, dirty)) in self.stash.iter_mut().filter(|(p, (_, d))| *d && !p.is_empty()) {
            match std::fs::write(path.as_str(), text.as_str()) {
                Ok(()) => {
                    *dirty = false;
                    self.notes.push(format!("saved {path}"));
                }
                Err(e) => self.notes.push(format!("cannot save {path}: {e}")),
            }
        }
    }

    /// The script the renderer runs, and its text as it stands: the editor's
    /// buffer when it is open there, saved or not, else the file.
    pub fn render_source(&self) -> Option<(String, String)> {
        let path = self.rendering.as_ref()?;
        if *path == self.script_path.trim() {
            return Some((path.clone(), self.script.clone()));
        }
        if let Some((text, _)) = self.stash.get(path) {
            return Some((path.clone(), text.clone()));
        }
        std::fs::read_to_string(path).ok().map(|text| (path.clone(), text))
    }

    /// Send `path` to the renderer, from a clean scene -- a new app's, its
    /// settings and camera too: a script run to its first iteration and held
    /// there, a Rust example built if it has to be and loaded, a mesh shown.
    /// The middle turns to the scene; the editor stays as it is.
    pub fn send_to_renderer(&mut self, path: &std::path::Path) {
        let key = path.display().to_string();
        self.reset_before_render = true;
        match path.extension().and_then(|e| e.to_str()) {
            Some("obj") => {
                // The scene is the mesh now: there is no script left to play.
                self.mesh_request = Some(path.to_path_buf());
                self.rendering = None;
            }
            Some("rs") => {
                self.rendering = Some(key);
                self.launch_request = true;
            }
            _ => {
                self.rendering = Some(key);
                self.run_request = true;
                self.restart_request = true;
            }
        }
        self.central_tab = CentralTab::Renderer;
    }

    /// A file the scripts tab renamed: its buffer, and the renderer's
    /// script, follow it.
    fn renamed(&mut self, from: &std::path::Path, to: &std::path::Path) {
        let (from, to) = (from.display().to_string(), to.display().to_string());
        self.buffers().renamed(&from, &to);
        if let Some(moved) = self.rendering.as_deref().and_then(|r| moved(r, &from, &to)) {
            self.rendering = Some(moved);
        }
    }

    /// What was thrown away, closed: the files open from under `gone`, and
    /// the renderer's script if it was one of them.
    fn thrown_away(&mut self, gone: &std::path::Path) {
        let under = |p: &str| std::path::Path::new(p).starts_with(gone);
        let open: Vec<String> = self.opened.iter().filter(|p| under(p)).cloned().collect();
        for p in open {
            self.close_file(&p);
        }
        if under(self.script_path.trim()) && !self.script_path.trim().is_empty() {
            let shown = self.script_path.trim().to_string();
            self.close_file(&shown);
        }
        if self.rendering.as_deref().is_some_and(under) {
            self.rendering = None;
        }
    }

    /// Fold the three docked panels to the window edges, or bring them all
    /// back. Each folded panel keeps egui's thin handle at its edge, so one
    /// can be dragged or double-clicked back out on its own -- the halfway
    /// house between the full layout and focus mode, which hides everything
    /// and reveals on hover.
    pub fn toggle_panels(&mut self) {
        let any_open = DOCKED.iter().any(|&i| self.docked_open[i]);
        for i in DOCKED {
            self.docked_open[i] = !any_open;
        }
    }

    /// Fold one docked panel or bring it back: `0` top, `1` bottom, `2`
    /// left, `3` right -- the order of `docked_open`. The arrow keys.
    pub fn toggle_panel(&mut self, side: usize) {
        if let Some(open) = self.docked_open.get_mut(side) {
            *open = !*open;
        }
    }

    pub fn new(
        window: &winit::window::Window,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> Self {
        let ctx = egui::Context::default();
        icons::install(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        let renderer = egui_wgpu::Renderer::new(
            device,
            format,
            egui_wgpu::RendererOptions {
                // The UI draws straight onto the swapchain, which has no
                // depth buffer and one sample. The scene's MSAA is the
                // scene's business -- it is resolved before egui sees it.
                depth_stencil_format: None,
                ..Default::default()
            },
        );
        Self {
            ctx,
            state,
            renderer,
            viewport_texture: None,
            registered_size: (0, 0),
            registered_generation: u64::MAX,
            panels: [egui::Rect::NOTHING; 4],
            // The left slot holds nothing; see `DOCKED`.
            docked_open: [true, true, false, true],
            last_folded: false,
            last_each: [false; 4],
            float_sizes: FLOAT_DEFAULTS,
            resizing: None,
            viewport_rect: egui::Rect::NOTHING,
            viewport_size: (
                window.inner_size().width.max(1),
                window.inner_size().height.max(1),
            ),
            log_tab: LogTab::Kalast,
            theme: None,
            console_input: String::new(),
            console_history: Vec::new(),
            console_back: 0,
            central_tab: CentralTab::Renderer,
            side_tab: SideTab::Simulation,
            sash_hover: None,
            sash_drag: None,
            files: FileTree::default(),
            opened: Vec::new(),
            stash: std::collections::HashMap::new(),
            rendering: None,
            mesh_request: None,
            reset_before_render: false,
            trash_asked: None,
            close_asked: None,
            notes: Vec::new(),
            script: String::new(),
            script_path: String::new(),
            script_dirty: false,
            script_editor: script::ScriptEditor::default(),
            docs: docs::Docs::default(),
            logo: None,
            badge: None,
            welcome_camera: None,
            welcome_gone: false,
            welcome_settle: 2,
            camera_asked: false,
            run_request: false,
            restart_request: false,
            confirm_exit: false,
            quit_request: false,
            exit_after_save: false,
            open_request: false,
            update_request: false,
            relaunch_request: false,
            save_request: false,
            rust_release: true,
            build_request: false,
            launch_request: false,
            rust_built: false,
            rust_key: (String::new(), false, 0),
            was_building: false,
            load_after_build: false,
            building: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Draw the UI for one frame, onto `surface_view`.
    ///
    /// Called after the scene has been rendered into `render_texture`, so the
    /// viewport panel has something to show. Returns the size the viewport
    /// panel wants the *next* scene render to be, in physical pixels.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        window: &winit::window::Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_view: &wgpu::TextureView,
        // Size of `surface_view`, which is **not** always the window's. A
        // fullscreen toggle resizes the window at once, while the swapchain
        // follows a frame or two later on the `Resized` event. egui laying
        // out for the window and drawing into the surface meant a scissor
        // rect wider than the target -- "Scissor Rect { w: 3024 } is not
        // contained in the render target (1200, 800)" -- which wgpu treats as
        // fatal.
        surface_size: (u32, u32),
        scene: &wgpu::Texture,
        scene_size: (u32, u32),
        scene_generation: u64,
        config: &mut crate::app::config::Config,
        app_config: &mut crate::app::config::AppConfig,
        sim: &mut crate::app::simulation::Simulation,
        shared: &mut crate::app::Shared,
        iteration_rate: f32,
        timestamps: Option<wgpu::RenderPassTimestampWrites<'_>>,
    ) -> (u32, u32) {
        // Re-register only when the texture behind it is a different one. A
        // `TextureId` outlives a resize, but the view it points at does not.
        if self.viewport_texture.is_none()
            || self.registered_size != scene_size
            || self.registered_generation != scene_generation
        {
            // A *non-sRGB* view of an sRGB texture: sampling returns the
            // stored bytes unchanged instead of converting them to linear.
            //
            // The scene is already encoded -- it is what an exported frame
            // contains, and what the plain window blits -- so egui must pass
            // it through, not decode it. With the default view the editor
            // showed every colour raised to the gamma: a flat 0.5 grey
            // measured 0.216 on screen, 0.5^2.2, against 0.502 in the plain
            // window and 0.502 in the exported PNG.
            let view = scene.create_view(&wgpu::TextureViewDescriptor {
                format: Some(scene.format().remove_srgb_suffix()),
                ..Default::default()
            });
            if let Some(id) = self.viewport_texture.take() {
                self.renderer.free_texture(&id);
            }
            self.viewport_texture =
                Some(self.renderer
                    .register_native_texture(device, &view, wgpu::FilterMode::Linear));
            self.registered_size = scene_size;
            self.registered_generation = scene_generation;
        }

        // Two panel closures both want the simulation -- the toolbar reads
        // the clock, the inspector shows everything else -- and both are
        // built before either runs. Only one is ever entered per frame, but
        // the borrow checker cannot see that, so the check moves to runtime.
        let sim = std::cell::RefCell::new(sim);

        // The panels' colours, set when they change.
        if self.theme != Some(app_config.theme) {
            theme::apply(&self.ctx, app_config.theme);
            self.theme = Some(app_config.theme);
        }

        // The screens, by name, for the app tab's list while the window is not
        // remembered -- and only then, since asking walks them all.
        let screens: Vec<String> = if app_config.remember_window {
            Vec::new()
        } else {
            window.available_monitors().map(|m| crate::app::screen_name(&m)).collect()
        };

        let mut raw = self.state.take_egui_input(window);
        // Lay out for what is being drawn into, not for the window.
        let ppp = self.ctx.pixels_per_point();
        let screen = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(surface_size.0 as f32 / ppp, surface_size.1 as f32 / ppp),
        );
        raw.screen_rect = Some(screen);
        let mut wanted = self.viewport_size;
        let mut vp_rect = egui::Rect::NOTHING;

        // The scene takes the window and the panels get out of the way,
        // each coming back when the pointer reaches its edge -- and staying
        // while the pointer is on it, which the edge test alone would not
        // give.
        //
        // Driven by `focus` and not by `fullscreen`: one is about what is
        // inside the window, the other about the window itself.
        let immersive = app_config.focus;
        let pointer = self.ctx.pointer_latest_pos();
        let mut shows = reveal_panels(immersive, pointer, screen, &self.panels);
        // Whatever is being dragged stays up, wherever the pointer has got to.
        if let Some(i) = self.resizing {
            shows[i] = true;
        }
        // The left edge summons nothing: the script is the editor tab.
        shows[2] = false;
        let [show_top, show_bottom, show_left, show_right] = shows;
        let mut rects = [egui::Rect::NOTHING; 4];
        let mut out_sizes = self.float_sizes;
        let mut out_resizing = self.resizing;
        let was_shown = self.panels.map(|r| r.is_positive());
        // A change on the config side -- a script before `start()`, the
        // Window header's checkbox -- folds or unfolds all four. The
        // per-panel state stays the editor's, since egui moves it by drag.
        if app_config.panels_folded != self.last_folded {
            for i in DOCKED {
                self.docked_open[i] = !app_config.panels_folded;
            }
        }
        // One panel from the config side -- `app.config.log_folded = True`
        // in a script, or its checkbox -- the same way.
        let folded_each = [
            app_config.toolbar_folded,
            app_config.log_folded,
            app_config.script_folded,
            app_config.simulation_folded,
        ];
        for i in DOCKED {
            if folded_each[i] != self.last_each[i] {
                self.docked_open[i] = !folded_each[i];
            }
        }
        let open_docked = self.docked_open;
        let mut out_open = self.docked_open;
        // Read before the panel closures are built: the toolbar needs to know
        // whether there is a script, the script panel needs the buffer, and
        // one cannot borrow it while the other holds it.
        // Something for Play to run: the script sent to the renderer.
        let has_script = self.rendering.is_some();
        // The edited files, for the question before quitting.
        let dirty_names = self.dirty_names();
        let confirm_exit = self.confirm_exit;
        shared.panels_shown = [show_top, show_bottom, show_left, show_right];
        shared.pointer = pointer.map(|p| (p.x, p.y));
        shared.ui_size = (screen.width(), screen.height());
        let texture_id = self.viewport_texture;
        // Read before the script's path is lent to the editor tab: the files
        // tab marks the open file with it. Relative to the working directory
        // when it is inside it, as the tree's paths are.
        let current_script = {
            let p = std::path::PathBuf::from(self.script_path.trim());
            std::env::current_dir()
                .ok()
                .and_then(|cwd| p.strip_prefix(cwd).ok().map(std::path::Path::to_path_buf))
                .unwrap_or(p)
        };
        // Chosen in the toolbar, read by the middle: two places at once, so a
        // cell, written back after the frame.
        let central_tab = std::cell::Cell::new(self.central_tab);
        // Which file the editor holds, named in the toolbar beside save. It
        // is opened from the files tab; the open button, its path field and
        // their dialog went when the tree came.
        let crumb = match self.script_path.trim() {
            "" => String::new(),
            p if shared.native => format!("{p}  (running)"),
            p => p.to_string(),
        };
        let side_tab = &mut self.side_tab;
        let files = &mut self.files;
        let sash_hover = &mut self.sash_hover;
        let sash_drag = &mut self.sash_drag;
        let accent = theme::accent(app_config.theme);
        let outline = theme::outline(app_config.theme);
        let side_fill = theme::side_fill(app_config.theme);
        let mut tree_action: Option<TreeAction> = None;
        // The side panel's open button, the editor's column and its buttons.
        let mut open_external = false;
        let mut nav_click: Option<NavClick> = None;
        let (mut render_now, mut preview_now) = (false, false);
        // What the editor's column lists: the files open, those edited, the
        // renderer's, and the shown one's outline.
        let opened = self.opened.clone();
        let edited: std::collections::HashSet<String> = self
            .stash
            .iter()
            .filter(|(_, (_, dirty))| *dirty)
            .map(|(path, _)| path.clone())
            .chain(self.script_dirty.then(|| self.script_path.trim().to_string()))
            .collect();
        let rendering = self.rendering.clone();
        let symbols = script::outline::symbols(&self.script, self.script_path.trim());
        // The prompts after a right click or a cross.
        let trash_asked = self.trash_asked.clone();
        let (mut trash_now, mut trash_cancel) = (false, false);
        let close_asked = self.close_asked.clone();
        let (mut close_saving, mut close_anyway, mut close_cancel) = (false, false, false);
        let mut remember = false;
        // An empty scene shows the logo, as VS Code's empty editor does.
        let scene_empty = sim.borrow().bodies.is_empty();
        // Taken every frame, so a drag made while the scene had bodies does
        // not dismiss the welcome that comes with emptying it.
        let camera_asked = std::mem::take(&mut self.camera_asked);
        if scene_empty && !self.welcome_gone {
            self.welcome_gone |= camera_asked;
            // And the camera moved some other way: the gizmo's plane views,
            // which turn it without a drag.
            let now = camera_state(&sim.borrow().camera);
            if self.welcome_settle > 0 {
                self.welcome_settle -= 1;
                self.welcome_camera = Some(now);
            } else if let Some(before) = self.welcome_camera {
                // A turn, a pan, a zoom, the gizmo: any of it, beyond the
                // rounding a frame's renormalising leaves -- single
                // precision's, some 1e-7, which a bound of 1e-9 took for a
                // move. The least a gesture turns is some 1e-3.
                let moved = before.iter().zip(now).any(|(a, b)| (a - b).abs() > 1e-4 * (1.0 + a.abs()));
                self.welcome_gone |= moved;
            } else {
                self.welcome_camera = Some(now);
            }
        }
        let welcome = self.ctx.animate_bool_with_time(
            egui::Id::new("kalast welcome"),
            scene_empty && !self.welcome_gone,
            0.25,
        );
        // The toolbar's logo, made again when the screen's density changes --
        // the window moved to another monitor.
        let ppp = self.ctx.pixels_per_point();
        if self.badge.as_ref().is_none_or(|(made_for, _)| *made_for != ppp) {
            self.badge = small_logo(&self.ctx, ppp).map(|t| (ppp, t));
        }
        let badge = self.badge.as_ref().map(|(_, t)| t.id());
        let logo = (welcome > 0.0).then(|| {
            self.logo
                .get_or_insert_with(|| {
                    let image = image::load_from_memory(LOGO).map(|i| i.into_rgba8()).unwrap_or_default();
                    let size = [image.width() as usize, image.height() as usize];
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                    self.ctx.load_texture("kalast logo", pixels, egui::TextureOptions::LINEAR)
                })
                .id()
        });
        let script = &mut self.script;
        let script_path = &mut self.script_path;
        let script_editor = &mut self.script_editor;
        let docs = &mut self.docs;
        let mut docs_request: Option<docs::Request> = None;
        // The editor's settings, read before the app tab can borrow them.
        let neovim_path = app_config.neovim_path.clone();
        let neovim_config = app_config.neovim_config.clone();
        let python_language_server = app_config.python_language_server.clone();
        let rust_language_server = app_config.rust_language_server.clone();
        let (neovim, ruler, language_servers) = (app_config.neovim, app_config.ruler, app_config.language_servers);
        let (mut editor_save, mut editor_open) = (false, None::<std::path::PathBuf>);
        let is_rust = rendering.as_deref().is_some_and(|p| p.trim_end().ends_with(".rs"));
        let editing_rust = script_path.trim_end().ends_with(".rs");
        let shown_renders = renderable(std::path::Path::new(script_path.trim()));
        let shown_markdown = docs::is_markdown(std::path::Path::new(script_path.trim()));
        let lang = code::Lang::of(script_path);
        let script_dirty = self.script_dirty;
        let script_ran = shared.script_ran;
        let drawn = shared.drawn_iteration;
        let update_state = shared.update.clone();
        let log = &mut shared.log;
        let kalast_log = &mut shared.kalast_log;
        let console_log = &mut shared.console_log;
        let console_input = &mut self.console_input;
        let console_history = &mut self.console_history;
        let console_back = &mut self.console_back;
        let log_tab = &mut self.log_tab;
        let dirty = &mut self.script_dirty;
        let ran = &mut shared.script_ran;
        let (mut run_request, mut save_request) = (false, false);
        let (mut update_request, mut relaunch_request) = (false, false);
        let (mut save_and_quit, mut quit_now, mut cancel_exit) = (false, false, false);
        // A Rust example is built and launched rather than run in this
        // process, so the transport buttons do not apply to one.
        let building = self.building.load(std::sync::atomic::Ordering::SeqCst);
        // The scene's clear colour, which is linear, as egui's sRGB: what a
        // viewport shows beyond the image while a panel slides.
        let scene_background = {
            let c = config.shading.background;
            egui::Color32::from(egui::Rgba::from_rgb(c.r as f32, c.g as f32, c.b as f32))
        };
        let rust_built = self.rust_built;
        let native = shared.native;
        let rust_release = &mut self.rust_release;
        let (mut build_request, mut launch_request) = (false, false);
        let (mut restart_request, mut reset_request) = (false, false);

        let mut output = self.ctx.run_ui(raw, |ui_root| {
            let ppp = ui_root.ctx().pixels_per_point();

            // The scene itself, drawn the same way in both layouts and
            // differing only in what it is given.
            let scene_ui = |ui: &mut egui::Ui, into: egui::Rect, corner: u8| {
                if let Some(id) = texture_id {
                    // At its own size, pixel for pixel, on the scene's
                    // background: the image was rendered for last frame's
                    // viewport, and while a panel slides open or shut that is
                    // a few pixels off this frame's. Fitted, it was rescaled
                    // every frame of the slide -- the scene pumped in and out
                    // -- and stretched it would distort; unscaled it only
                    // shows or hides a sliver at the edge for one frame, the
                    // same colour as the sky around the bodies.
                    let size = egui::vec2(scene_size.0 as f32, scene_size.1 as f32) / ppp;
                    let clip = into.intersect(ui.clip_rect());
                    ui.painter()
                        .with_clip_rect(clip)
                        .rect_filled(into, egui::CornerRadius::same(corner), scene_background);
                    // Painted into a rect worked out here, not laid out by
                    // the `Ui`. An `Area` is unbounded, so asking it to centre
                    // something centres it in an infinite region -- which put
                    // the scene in the bottom-right corner of the window,
                    // mostly out of view.
                    let previous = ui.clip_rect();
                    ui.set_clip_rect(clip);
                    egui::Image::new(egui::load::SizedTexture::new(id, size))
                        .corner_radius(egui::CornerRadius::same(corner))
                        .paint_at(ui, egui::Rect::from_center_size(into.center(), size));
                    ui.set_clip_rect(previous);
                }
                if let Some(logo) = logo {
                    watermark(ui, into, logo, welcome);
                }
            };

            // Each panel's contents, named once so the same code can go in a
            // side panel or a floating one.
            let toolbar = app_config.toolbar.clone();
            let ui_theme = app_config.theme;
            let toolbar_ui = |ui: &mut egui::Ui| {
                let mut sim = sim.borrow_mut();
                let sim = &mut **sim;
                let diagnostics = &sim.diagnostics;
                let state = &mut sim.state;
                let colors = theme::actions(ui_theme);
                // Play is the only way to start. A separate Run was the same
                // button twice: both meant "go", and you had to press one
                // then find the other.
                //
                // What it does depends on where the script stands. With
                // nothing loaded there is nothing to play, so it is dead
                // rather than advancing a clock nobody reads. With a script
                // not yet run -- or edited since it last ran -- it runs it,
                // which starts the simulation as a side effect. After that it
                // is transport.
                // Native: this window *is* a compiled example, launched by an
                // editor. There is no script to run -- the program is already
                // running -- so Play is a pause toggle.
                // A Rust example that has been loaded behaves like a native
                // window: it is running here, so Play is its pause button and
                // Restart rebuilds its scene.
                let loaded = is_rust && script_ran;
                let have_script = if native || loaded {
                    true
                } else if is_rust {
                    // Nothing to load until it has been compiled.
                    rust_built
                } else {
                    has_script
                };
                let (pausing, hover): (bool, &str) = if native || loaded {
                    if state.is_paused {
                        (false, "Play: resume  (P)")
                    } else {
                        (true, "Pause: hold the simulation  (P)")
                    }
                } else if is_rust {
                    if rust_built {
                        (false, "Play: load this example into this window")
                    } else {
                        (false, "Play: compile it first")
                    }
                } else if !have_script {
                    (false, "Play: open or write a script first")
                } else if !script_ran {
                    (false, "Play: run this script and start the simulation  (P)")
                } else if state.is_paused {
                    (false, "Play: resume  (P)")
                } else {
                    (true, "Pause: hold the simulation  (P)")
                };
                let (icon, color) = if pausing {
                    (icons::codicon::PAUSE, colors.step)
                } else {
                    (icons::codicon::PLAY, colors.run)
                };
                // The file on the left; on the right the readout and then the
                // actions that run it, as VS Code's editor title bar has its
                // run button at its right end. The right side is laid out
                // first and the left gets what remains, a long path cut short
                // with an ellipsis rather than pushing the actions off the
                // window.
                egui::containers::Sides::new().height(ACTION.y).shrink_left().truncate().show(
                    ui,
                    |ui| {
                        // kalast's logo first, where VS Code has its own.
                        if let Some(badge) = badge {
                            let size = egui::vec2(BADGE, BADGE);
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(badge, size)))
                                .on_hover_text(concat!("kalast v", env!("CARGO_PKG_VERSION")));
                            ui.add_space(4.0);
                        }
                        // What the middle shows, the scene or the script. Here
                        // rather than on the middle, so folding the toolbar --
                        // `↑` -- leaves nothing but the scene.
                        let tab = central_tab.get();
                        if panel_tab(ui, tab == CentralTab::Renderer, "renderer", false, accent)
                            .on_hover_text("The scene")
                            .clicked()
                        {
                            central_tab.set(CentralTab::Renderer);
                        }
                        if panel_tab(
                            ui,
                            tab == CentralTab::Editor,
                            // VS Code's dot for an edited file: the Codicon,
                            // since the UI font has no U+25CF and drew a box.
                            if script_dirty { "editor \u{ea71}" } else { "editor" },
                            false,
                            accent,
                        )
                        .on_hover_text(if script_dirty { "The script, edited since it was saved" } else { "The script" })
                        .clicked()
                        {
                            central_tab.set(CentralTab::Editor);
                        }
                        if panel_tab(ui, tab == CentralTab::Docs, "documentation", false, accent)
                            .on_hover_text("kalast's documentation: the Python API, the config, the controls, the changelog")
                            .clicked()
                        {
                            central_tab.set(CentralTab::Docs);
                        }
                        if ui
                            .add_enabled(script_dirty, action(icons::codicon::SAVE, ui.visuals().text_color()))
                            .on_hover_text("Save: write the script back to its file")
                            .clicked()
                        {
                            save_request = true;
                        }
                        if !crumb.is_empty() {
                            ui.label(egui::RichText::new(&crumb).weak()).on_hover_text(&crumb);
                        }
                        // A newer release, when the check that ran as the UI
                        // app opened found one: its notes are in the log.
                        use crate::app::update::State;
                        match &update_state {
                            State::Available(u) => {
                                ui.separator();
                                if ui
                                    .add(primary(ui_theme, icons::codicon::CLOUD_DOWNLOAD, &format!("Update to {}", u.latest.tag)))
                                    .on_hover_text("Download this release for this machine and install it in place; its notes are in the log")
                                    .clicked()
                                {
                                    update_request = true;
                                }
                            }
                            State::Installing => {
                                ui.separator();
                                ui.label("updating\u{2026}");
                            }
                            State::Ready => {
                                ui.separator();
                                if ui
                                    .add(primary(ui_theme, icons::codicon::REFRESH, "Restart kalast"))
                                    .on_hover_text("Start kalast again, on the new version, with this command line")
                                    .clicked()
                                {
                                    relaunch_request = true;
                                }
                            }
                            State::Failed(_) => {
                                ui.separator();
                                ui.label("update failed, see the log");
                            }
                            State::Unchecked | State::Checking | State::UpToDate => {}
                        }
                    },
                    |ui| {
                        // Right to left: reset at the window's edge, then
                        // step, restart, play, and the readout before them.
                        //
                        // Back to nothing: no bodies, a new app's settings and
                        // camera, no script running -- the one in the editor
                        // stays, for Play -- and the empty scene's welcome.
                        // Always there, an empty scene included: it was greyed
                        // until something had been loaded, and a camera turned
                        // in the empty scene had no quick way back. Not in a
                        // native window, which is the example itself.
                        if ui
                            .add_enabled(!native, action(icons::codicon::CLEAR_ALL, colors.stop))
                            .on_hover_text("Reset: back to a new app -- no bodies, its settings and camera, the welcome")
                            .clicked()
                        {
                            reset_request = true;
                        }
                        // One frame while paused: the same thing the render
                        // loop does, so the button cannot drift from the key.
                        if ui
                            .add_enabled(
                                // Anything that has actually run can be
                                // stepped: a hosted script, a native window, or
                                // a Rust example loaded into this one.
                                (script_ran || native) && state.is_paused,
                                action(icons::codicon::STEP, colors.step),
                            )
                            .on_hover_text("Step: advance one iteration  (K)")
                            .clicked()
                        {
                            state.is_paused = false;
                            state.hold_after_iteration = Some(state.iteration);
                        }
                        // Enabled whenever there is something to run, not only
                        // once it has run: an edit clears `script_ran` so that
                        // Play means "run the new text", and that greyed
                        // Restart out at exactly the moment it was wanted -- to
                        // see the change. It runs the text in the panel, saved
                        // or not; saving is for when the change is worth
                        // keeping.
                        if ui
                            .add_enabled(have_script && !native, action(icons::codicon::RESTART, colors.run))
                            .on_hover_text(if native {
                                "Restart: this window is the example; close it and launch again"
                            } else if is_rust {
                                "Restart: load this example again and stop at the start"
                            } else {
                                "Restart: rebuild the scene from the text in the panel -- saved or not -- and stop at the start"
                            })
                            .clicked()
                        {
                            // Reloading *is* the restart: it clears the scene
                            // and runs the example again from the top. A `.rs`
                            // never reaches the script runner, so for one this
                            // is Play.
                            if is_rust {
                                launch_request = true;
                            } else {
                                run_request = true;
                                restart_request = true;
                            }
                        }
                        if ui.add_enabled(have_script, action(icon, color)).on_hover_text(hover).clicked() {
                            if native || loaded {
                                state.is_paused = !state.is_paused;
                            } else if is_rust {
                                launch_request = true;
                            } else if script_ran {
                                state.is_paused = !state.is_paused;
                            } else {
                                // Running unpauses; see `serve_editor_requests`.
                                run_request = true;
                            }
                        }
                        // The same template a HUD takes, so the toolbar says
                        // whatever this run wants it to -- and `{drawn}` rather
                        // than `{it}` by default, because once the frame for
                        // iteration 0 is drawn `state.iteration` is already 1,
                        // and "1" under a picture of 0 is a lie of one frame.
                        if !toolbar.is_empty() {
                            ui.label(crate::app::expand_hud(
                                &toolbar,
                                state,
                                iteration_rate as crate::Float,
                                diagnostics,
                                drawn,
                            ))
                            .on_hover_text(
                                "app.config.toolbar -- {drawn} {it} {its} {fps} {ms} {bodies} {paused} {warn} {gpu}",
                            );
                        }
                        // What the transport acts on, whatever file the
                        // editor shows: the script sent to the renderer.
                        if let Some(sent) = rendering.as_deref() {
                            let name = std::path::Path::new(sent).file_name().map_or(sent.into(), |n| n.to_string_lossy());
                            ui.label(egui::RichText::new(format!("{}  {name}", icons::codicon::SEND)).weak())
                                .on_hover_text(format!("{sent}\nsent to the renderer: Play, Restart and Step act on it"));
                        }
                    },
                );
            };
            let log_ui = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        // The tab not shown gets a dot while it has lines not
                        // seen yet. Painted on its corner rather than added to
                        // its label, so the row does not shift as it comes and
                        // goes.
                        let dot = ui.visuals().hyperlink_color;
                        for (tab, name, hover, unread) in [
                            (
                                LogTab::Kalast,
                                "kalast",
                                "What kalast prints: loading, the update check, builds, pauses, debug output",
                                kalast_log.has_unread(),
                            ),
                            (
                                LogTab::Script,
                                "script",
                                "What the script prints: print, tracebacks, app.log",
                                log.has_unread(),
                            ),
                            (
                                LogTab::Python,
                                "python",
                                "A Python console: a line runs between frames, among the script's variables",
                                console_log.has_unread(),
                            ),
                        ] {
                            let chosen = *log_tab == tab;
                            let response = panel_tab(ui, chosen, name, true, accent).on_hover_text(hover);
                            if response.clicked() {
                                *log_tab = tab;
                            }
                            let tab_rect = response.rect;
                            if unread && *log_tab != tab {
                                ui.painter().circle_filled(tab_rect.right_top() + egui::vec2(0.0, 3.0), 2.5, dot);
                            }
                        }
                        if ui
                            .add(action(icons::codicon::CLEAR_ALL, ui.visuals().text_color()))
                            .on_hover_text("Clear this tab  (Ctrl+L at the python prompt)")
                            .clicked()
                        {
                            match *log_tab {
                                LogTab::Script => log.clear(),
                                LogTab::Kalast => kalast_log.clear(),
                                LogTab::Python => console_log.clear(),
                            }
                        }
                    });
                    // The console reads as a transcript, without stamps.
                    let (shown, stamped) = match *log_tab {
                        LogTab::Kalast => (&mut *kalast_log, true),
                        LogTab::Script => (&mut *log, true),
                        LogTab::Python => (&mut *console_log, false),
                    };
                    // What the shown tab has, it has shown.
                    shown.mark_read();
                    // A scroll position per tab, so switching does not drop
                    // one at the other's place.
                    //
                    // Filling the panel, not shrunk to the lines: a docked
                    // panel keeps the height its content used, so a tab with
                    // fewer lines shrank it on switching, and a drag taller
                    // than the text snapped back on release.
                    let tab = *log_tab;
                    let palette = code::palette(ui_theme);
                    // Ctrl+L clears the python tab, as it clears a terminal.
                    if tab == LogTab::Python
                        && ui.memory(|m| m.has_focus(egui::Id::new("console_input")))
                        && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L))
                    {
                        shown.clear();
                    }
                    egui::ScrollArea::vertical()
                        .id_salt(("log", tab))
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                transcript(ui, shown, stamped, &palette);
                                // The python tab's prompt follows its last line,
                                // as a terminal's does, rather than sitting at
                                // the panel's foot below an empty screen. Inside
                                // the scroll area, which fills the panel whatever
                                // its content: a prompt below it, sized from a
                                // guess at its height, ran a point past the panel
                                // whenever the guess was short, egui kept the
                                // taller panel, and it grew every frame to its
                                // maximum.
                                if tab == LogTab::Python {
                                    console_prompt(ui, console_input, console_history, console_back, &palette);
                                }
                            });
                        });
                };
            let script_ui = |ui: &mut egui::Ui| {
                // The files open and the shown one's outline, in a column on
                // the left, as the documentation has its pages and theirs.
                let nav = egui::Panel::left("editor nav")
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin { left: 0, right: 8, top: 0, bottom: 0 }))
                    .show_separator_line(false)
                    .resizable(true)
                    .default_size(210.0)
                    .size_range(140.0..=420.0)
                    .show(ui, |ui| editor_nav(ui, ui_theme, &opened, script_path.trim(), &edited, rendering.as_deref(), &symbols));
                nav_click = nav.inner;
                let edge = nav.response.rect;
                ui.painter().vline(edge.right(), edge.y_range(), egui::Stroke::new(1.0, outline));
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    // The shown file's own buttons, on the right as VS Code
                    // has an editor's. A Rust example is a separate program:
                    // it links kalast as a library, so it is built -- a bug
                    // for debug, a rocket for release, the chevron to choose
                    // -- and loaded, never run in this process. Render sends
                    // what is shown to the renderer: nothing else does. Not
                    // in a launched example, which is already running it.
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let text = ui.visuals().text_color();
                            if shown_markdown
                                && ui
                                    .add(action(icons::codicon::OPEN_PREVIEW, text))
                                    .on_hover_text("Open as documentation: this file as a page of the documentation tab")
                                    .clicked()
                            {
                                preview_now = true;
                            }
                            if ui
                                .add_enabled(shown_renders && !native, action(icons::codicon::SEND, accent))
                                .on_hover_text(
                                    "Render: send this file to the renderer -- a script run to its first iteration, \
                                     a Rust example built and loaded, a mesh shown. Opening a file never does.",
                                )
                                .on_disabled_hover_text("Render: for a Python script, a Rust example or a mesh")
                                .clicked()
                            {
                                render_now = true;
                            }
                            let rust = editing_rust && !native;
                            ui.add_enabled_ui(rust, |ui| {
                                ui.menu_button(egui::RichText::new(icons::codicon::CHEVRON_DOWN).size(12.0), |ui| {
                                    ui.radio_value(rust_release, false, "debug").on_hover_text("cargo build --example ...");
                                    ui.radio_value(rust_release, true, "release").on_hover_text(
                                        "cargo build --release --example ... -- 2-15x faster here, and what any run worth keeping wants",
                                    );
                                })
                                .response
                                .on_hover_text("debug or release");
                            });
                            let (mode, profile) = if *rust_release {
                                (icons::codicon::ROCKET, "release")
                            } else {
                                (icons::codicon::DEBUG, "debug")
                            };
                            if ui
                                .add_enabled(rust && !building, action(mode, text))
                                .on_hover_text(if building {
                                    "a compile is already running".to_string()
                                } else {
                                    format!("Compile: cargo build for this example, {profile}")
                                })
                                .on_disabled_hover_text("Compile: for a Rust example")
                                .clicked()
                            {
                                build_request = true;
                            }
                        });
                    });
                    // The editor: VS Code's, with a language server's
                    // completion and hover, or the user's Neovim. See
                    // `script`.
                    let palette = code::palette(ui_theme);
                    let settings = script::Settings {
                        neovim,
                        neovim_path: &neovim_path,
                        neovim_config: &neovim_config,
                        ruler,
                        language_servers,
                        python_language_server: &python_language_server,
                        rust_language_server: &rust_language_server,
                        theme: ui_theme,
                    };
                    let outcome =
                        script_editor.show(ui, script, script_path.as_str(), lang, &palette, &settings, *dirty);
                    if outcome.changed {
                        *dirty = true;
                        // What is running is no longer what is shown -- when
                        // what is shown is what is running.
                        if rendering.as_deref() == Some(script_path.trim()) {
                            *ran = false;
                        }
                    }
                    // Under Neovim, measured against the file: `u` back to
                    // the saved text is not an edit to save.
                    if let Some(modified) = outcome.modified {
                        *dirty = modified;
                    }
                    editor_save |= outcome.save;
                    if outcome.quit {
                        central_tab.set(CentralTab::Renderer);
                    }
                    if outcome.open.is_some() {
                        editor_open = outcome.open;
                    }
                });
            };
            // kalast's references, rendered; a link followed from them is
            // served after the frame. See `docs`.
            let docs_ui = |ui: &mut egui::Ui| {
                docs_request = docs.show(ui, ui_theme);
            };
            let config_ui = |ui: &mut egui::Ui| {
                    // Icons, as VS Code's activity bar has them, named on
                    // hover. No rule under them: the accent under the chosen
                    // one is the division, and a line across the panel was a
                    // bar between the tabs and what they show.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (which, icon, hover) in [
                            (SideTab::App, icons::codicon::SETTINGS_GEAR, "App: the app's own settings, remembered for next time"),
                            (SideTab::Simulation, icons::codicon::GLOBE, "Simulation: app.simulation.config, and the scene's bodies, camera and Sun"),
                            (
                                SideTab::Files,
                                icons::codicon::FILE_CODE,
                                "Scripts: the bundle's examples and your own scripts; a click opens a file in the editor, a right click does the rest",
                            ),
                        ] {
                            if widgets::icon_tab(ui, *side_tab == which, icon, accent).on_hover_text(hover).clicked() {
                                *side_tab = which;
                            }
                        }
                        // Any file, from anywhere, as the system's dialog finds it.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(action(icons::codicon::FOLDER_OPENED, ui.visuals().text_color()))
                                .on_hover_text("Open a file: any text file, from anywhere -- a script, a mesh, a README")
                                .clicked()
                            {
                                open_external = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    // A scroll position per tab, so switching does not drop
                    // one at the other's place.
                    egui::ScrollArea::vertical()
                        .id_salt(("side", *side_tab))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // All three tabs on the files tab's grid: rows of
                            // its height, no gap between them.
                            ui.spacing_mut().item_spacing.y = 0.0;
                            match *side_tab {
                            SideTab::App => {
                                use theme::palette;
                                let before = crate::app::settings::Remembered::of(app_config);
                                use widgets::Icon::Codicon;
                                widgets::section(ui, Codicon(icons::codicon::LAYOUT, palette::BLUE), "Panels", "Panels", |ui| {
                                    config_panel::app_panels(ui, app_config)
                                });
                                widgets::section(ui, Codicon(icons::codicon::WINDOW, palette::LAVENDER), "Window", "Window", |ui| {
                                    config_panel::app_window(ui, app_config);
                                    if !app_config.remember_window {
                                        window_start(ui, app_config, &screens);
                                    }
                                });
                                widgets::section(ui, Codicon(icons::codicon::CODE, palette::GREEN), "Editor", "Editor", |ui| {
                                    config_panel::app_editor(ui, app_config)
                                });
                                widgets::section(
                                    ui,
                                    Codicon(icons::codicon::CLOUD_DOWNLOAD, palette::SAPPHIRE),
                                    "Updates",
                                    "Updates",
                                    |ui| config_panel::app_updates(ui, app_config),
                                );
                                remember |= crate::app::settings::Remembered::of(app_config) != before;
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new("The theme, the window and the editor's settings are remembered for next time.")
                                        .weak()
                                        .small(),
                                );
                            }
                            // By topic, each header holding the entity beside
                            // its own settings -- the Sun beside its light, the
                            // HUD list beside its font. See `simulation_panel`.
                            SideTab::Simulation => {
                                let mut sim = sim.borrow_mut();
                                // The config is passed in rather than read from
                                // `sim.config`: that is the same RefCell this
                                // panel is being drawn with open, and reading it
                                // here panics.
                                simulation_panel::simulation_panel(ui, &mut sim, config);
                            }
                            SideTab::Files => {
                                // Headed as VS Code heads its explorer.
                                ui.label(egui::RichText::new("SCRIPTS").strong().size(11.0));
                                tree_action = files.show(ui, &current_script, ui_theme);
                            }
                            }
                        });
                };

            // Floating panels get their own layers, which egui paints *above*
            // the root -- where side panels and the central panel live.
            //
            // That ordering is why the scene cannot just go in a background
            // `Area` instead: the root layer is painted first whatever order
            // an `Area` asks for, so the scene covered the panels and nothing
            // appeared at any edge, however early the `Area` was created.
            //
            // So the scene stays in the central panel and the panels float
            // over it. In focus mode no side panel takes anything, so the
            // central panel is the whole window and revealing one does not
            // resize the render target.
            let (screen_w, screen_h) = (screen.width(), screen.height());
            let float = |ctx: &egui::Context,
                         id: &'static str,
                         rect: egui::Rect,
                         side: usize,
                         size: &mut f32,
                         resizing: &mut Option<usize>,
                         add: &mut dyn FnMut(&mut egui::Ui)| -> egui::Rect {
                egui::Area::new(id.into())
                    .order(egui::Order::Foreground)
                    .fixed_pos(rect.min)
                    .show(ctx, |ui| {
                        // The toolbar is one row and takes that row's height.
                        // Given `FLOAT_DEFAULTS[0]` and made to fill it, it
                        // wore a band of empty frame under the buttons, and a
                        // bar half again as tall as its docked self read as
                        // two rows. Only its width is pinned, so it spans the
                        // window like the docked one. The other three fill
                        // the rect they are given, and can be dragged.
                        let toolbar = side == 0;
                        if toolbar {
                            ui.set_max_width(rect.width());
                        } else {
                            ui.set_max_size(rect.size());
                        }
                        let framed = egui::Frame::popup(ui.style()).show(ui, |ui| {
                            if toolbar {
                                ui.set_min_width(rect.width());
                            } else {
                                ui.set_min_size(rect.size());
                            }
                            add(ui);
                        });
                        if toolbar {
                            return;
                        }

                        // A grab strip on the inner edge, standing in for the
                        // resize handle a docked panel has and an `Area` does
                        // not.
                        //
                        // Measured from where the panel actually ended up, not
                        // from the rect it was asked for: the frame adds its
                        // own margins and a short panel does not fill the
                        // height it was given, so a handle placed from the
                        // request sat away from the edge it belongs to.
                        let actual = framed.response.rect;
                        const GRAB: f32 = 6.0;
                        let strip = match side {
                            1 => egui::Rect::from_min_max(
                                actual.min,
                                egui::pos2(actual.right(), actual.top() + GRAB),
                            ),
                            2 => egui::Rect::from_min_max(
                                egui::pos2(actual.right() - GRAB, actual.top()),
                                actual.max,
                            ),
                            _ => egui::Rect::from_min_max(
                                actual.min,
                                egui::pos2(actual.left() + GRAB, actual.bottom()),
                            ),
                        };
                        let grab = ui.interact(
                            strip,
                            ui.id().with("grab"),
                            egui::Sense::drag(),
                        );
                        if grab.hovered() || grab.dragged() {
                            ui.ctx().set_cursor_icon(if side < 2 {
                                egui::CursorIcon::ResizeVertical
                            } else {
                                egui::CursorIcon::ResizeHorizontal
                            });
                        }
                        if grab.dragged() {
                            let d = grab.drag_delta();
                            *size += match side {
                                1 => -d.y,
                                2 => d.x,
                                _ => -d.x,
                            };
                            // Never past the window, and never negative --
                            // dragged shut is a legitimate place to leave one.
                            // Down to nothing is allowed: that is what
                            // putting one away looks like here.
                            let limit = if side < 2 { screen_h } else { screen_w };
                            *size = size.clamp(0.0, limit);
                            *resizing = Some(side);
                        }
                        if grab.drag_stopped() {
                            *resizing = None;
                        }
                    })
                    .response
                    .rect
            };

            // Remembered between reveals, so a panel dragged wider stays
            // wider the next time the pointer summons it.
            let mut sizes = self.float_sizes;
            let mut resizing = self.resizing;

            // A floating panel can be dragged away to nothing, like a docked
            // one. Unlike a docked one it has no handle left behind to drag
            // back -- it is summoned by the pointer instead -- so a panel that
            // was put away comes back at its usual size when it is next
            // asked for. Without this, dragging one to nothing hid it for
            // good: every later reveal showed a panel zero points wide.
            //
            // Only as it reappears, judged by whether it was drawn last
            // frame, or it would spring back under the hand that shrank it.
            for i in 0..4 {
                let reappearing = !was_shown[i];
                if reappearing && sizes[i] < COLLAPSE {
                    sizes[i] = FLOAT_DEFAULTS[i];
                }
            }
            let [top_h, bottom_h, _, right_w] = sizes;

            let scene_panel = |ui_root: &mut egui::Ui,
                               vp_rect: &mut egui::Rect,
                               wanted: &mut (u32, u32)| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui_root, |ui| {
                        *vp_rect = ui.available_rect_before_wrap();
                        *wanted = (
                            ((vp_rect.width() * ppp).round() as u32).max(1),
                            ((vp_rect.height() * ppp).round() as u32).max(1),
                        );
                        scene_ui(ui, *vp_rect, 0);
                    });
            };

            if immersive {
                // The scene, the whole window -- or the script or the
                // documentation, when the toolbar's tabs asked for them.
                match central_tab.get() {
                    CentralTab::Editor => {
                        egui::CentralPanel::default().show(ui_root, script_ui);
                    }
                    CentralTab::Docs => {
                        egui::CentralPanel::default().show(ui_root, docs_ui);
                    }
                    CentralTab::Renderer => scene_panel(ui_root, &mut vp_rect, &mut wanted),
                }

                let ctx = ui_root.ctx().clone();
                let mut toolbar_ui = toolbar_ui;
                let mut log_ui = log_ui;
                let mut config_ui = config_ui;
                if show_top {
                    let r =
                        egui::Rect::from_min_size(screen.min, egui::vec2(screen.width(), top_h));
                    rects[0] = float(&ctx, "toolbar", r, 0, &mut sizes[0], &mut resizing, &mut toolbar_ui);
                }
                if show_bottom {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(screen.left(), screen.bottom() - bottom_h),
                        egui::vec2(screen.width(), bottom_h),
                    );
                    rects[1] = float(&ctx, "log", r, 1, &mut sizes[1], &mut resizing, &mut log_ui);
                }
                if show_right {
                    let r = egui::Rect::from_min_size(
                        egui::pos2(screen.right() - right_w, screen.top()),
                        egui::vec2(right_w, screen.height()),
                    );
                    rects[3] = float(&ctx, "config", r, 3, &mut sizes[3], &mut resizing, &mut config_ui);
                }
                out_sizes = sizes;
                out_resizing = resizing;
            } else {
                // The toolbar folds like the others, from its lower edge:
                // dragged up past egui's 20-point minimum, or double-clicked,
                // and the handle left at the top edge brings it back. It is
                // `resizable` only because that is what gives a panel its
                // handle. It cannot actually be made taller: a panel is the
                // size of its content, and one row of buttons does not
                // stretch to fill a drag the way a scroll area does.
                // VS Code's cards: the panels and the middle each a rounded
                // card, apart from one another over the theme's darkest
                // shade, and the toolbar straight on it, a title bar. No
                // idle separator line along a panel's edge -- in the gap it
                // read as a pale border between the cards -- only the
                // highlight egui draws while the edge is hovered or dragged.
                let visuals = ui_root.visuals().clone();
                ui_root.painter().rect_filled(screen, 0.0, visuals.extreme_bg_color);
                let card = egui::Frame::new()
                    .fill(visuals.panel_fill)
                    // One physical pixel, as VS Code's is: a point is two
                    // of them on a Retina screen, and read twice as heavy.
                    .stroke(egui::Stroke::new(1.0 / ppp, outline))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(8))
                    .outer_margin(egui::Margin::same(CARD_MARGIN));
                // egui lights a panel's edge itself while it is hovered or
                // dragged: 1 px against the card, in the text colour. It reads
                // the strokes from the style the panel is shown in, so the
                // panels are shown with those at no width, their contents
                // given the style back, and VS Code's sash drawn instead.
                let style = ui_root.style().clone();
                {
                    let w = &mut ui_root.style_mut().visuals.widgets;
                    w.hovered.fg_stroke.width = 0.0;
                    w.active.fg_stroke.width = 0.0;
                }
                let restyle = |ui: &mut egui::Ui| ui.set_style(style.clone());
                let (mut toolbar_ui, mut config_ui, mut log_ui) = (toolbar_ui, config_ui, log_ui);
                let mut open = open_docked;
                rects[0] = egui::Panel::top("toolbar")
                    .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 4)))
                    .show_separator_line(false)
                    .resizable(true)
                    .show_collapsible(ui_root, &mut open[0], move |ui: &mut egui::Ui| {
                        restyle(ui);
                        toolbar_ui(ui)
                    })
                    .map(|r| r.response.rect)
                    .unwrap_or(egui::Rect::NOTHING);
                // `show_collapsible`, not `show`: dragging a resize handle
                // past the minimum shuts the panel, and a thin handle stays at
                // the window edge to drag it back -- the way an editor's side
                // bars work. Double clicking the edge toggles it too.
                //
                // `min_size` is doing two jobs, because egui does not let
                // them be separated: it is the smallest a panel can be
                // dragged *and* the size below which it collapses. egui has a
                // `collapse_threshold` for exactly this, but it is private.
                //
                // A comfortable size rather than a small one, tried both
                // ways round and preferred like this: a panel keeps a usable
                // width for as long as it is open, and shutting it is one
                // decisive drag rather than a slow squeeze through sizes
                // nothing can be read at.
                //
                // The floating panels use `COLLAPSE` instead. They are not
                // the same question: those are summoned and dismissed by the
                // pointer already, so shrinking one is about the size it will
                // have next time, not about getting rid of it.
                rects[3] = egui::Panel::right("config")
                    // A shade darker than the other cards, as VS Code's
                    // sidebar is.
                    .frame(card.fill(side_fill))
                    .show_separator_line(false)
                    .resizable(true)
                    .default_size(right_w)
                    .min_size(180.0)
                    .show_collapsible(ui_root, &mut open[3], move |ui: &mut egui::Ui| {
                        restyle(ui);
                        config_ui(ui)
                    })
                    .map(|r| r.response.rect)
                    .unwrap_or(egui::Rect::NOTHING);
                // After the side panels, which then run the full height: a
                // panel takes the whole edge of whatever is left when it is
                // added, so the log sits between them, under the scene, the
                // way VS Code lays out its side bars and its panel.
                rects[1] = egui::Panel::bottom("log")
                    .frame(card)
                    .show_separator_line(false)
                    .resizable(true)
                    .default_size(bottom_h)
                    .min_size(120.0)
                    .show_collapsible(ui_root, &mut open[1], move |ui: &mut egui::Ui| {
                        restyle(ui);
                        log_ui(ui)
                    })
                    .map(|r| r.response.rect)
                    .unwrap_or(egui::Rect::NOTHING);
                out_open = open;
                ui_root.set_style(style.clone());

                // The middle: the scene, the script or the documentation, as
                // the toolbar's tabs choose. The scene is measured only while
                // it is the one shown, so the render keeps its size while the
                // others are, and a click on them is not a click on it.
                // The scene in no card: it fills the middle to its edges, so
                // with the panels folded it is the whole window, as a render
                // window is. The others take a card, like the panels.
                let mut script_ui = script_ui;
                let mut docs_ui = docs_ui;
                let frame = match central_tab.get() {
                    CentralTab::Renderer => egui::Frame::NONE,
                    CentralTab::Editor | CentralTab::Docs => card,
                };
                egui::CentralPanel::default().frame(frame).show(ui_root, |ui| {
                    match central_tab.get() {
                        CentralTab::Renderer => {
                            vp_rect = ui.available_rect_before_wrap();
                            wanted = (
                                ((vp_rect.width() * ppp).round() as u32).max(1),
                                ((vp_rect.height() * ppp).round() as u32).max(1),
                            );
                            scene_ui(ui, vp_rect, 0);
                        }
                        CentralTab::Editor => script_ui(ui),
                        CentralTab::Docs => docs_ui(ui),
                    }
                });

                // The middle of a gap, which is not always egui's edge: a
                // card's gap is both cards' margins around the edge, but the
                // scene has none, so beside it the gap is the panel's margin
                // alone and its middle half of that into the panel. (VS Code's
                // three-dot grip sat there too, and was taken out again: it
                // looked out of place. The edges drag the same without it.)
                let half = f32::from(CARD_MARGIN) / 2.0;
                let inset = if central_tab.get() == CentralTab::Renderer { half } else { 0.0 };

                // VS Code's sash: a draggable edge lit in the theme's accent,
                // 4 px down the middle of the gap -- once the pointer has
                // rested on it `SASH_DELAY`, and at once while it is dragged.
                // egui's grab zone is 3 px either side of the edge, which
                // takes in the gap; a folded panel's is the window's edge.
                let top = if open[0] && rects[0].is_positive() { rects[0].bottom() } else { screen.top() };
                let right = if open[3] && rects[3].is_positive() { rects[3].left() } else { screen.right() };
                let sashes = [
                    (0, if open[0] && rects[0].is_positive() {
                        egui::Rect::from_center_size(
                            egui::pos2(screen.center().x, rects[0].bottom() + half),
                            egui::vec2(screen.width(), 4.0),
                        )
                    } else {
                        egui::Rect::from_min_max(screen.left_top(), egui::pos2(screen.right(), screen.top() + 4.0))
                    }),
                    (1, if open[1] && rects[1].is_positive() {
                        egui::Rect::from_center_size(
                            egui::pos2(rects[1].center().x, rects[1].top() + inset),
                            egui::vec2(rects[1].width(), 4.0),
                        )
                    } else {
                        egui::Rect::from_min_max(egui::pos2(screen.left(), screen.bottom() - 4.0), egui::pos2(right, screen.bottom()))
                    }),
                    (3, if open[3] && rects[3].is_positive() {
                        egui::Rect::from_center_size(
                            egui::pos2(rects[3].left() + inset, rects[3].center().y),
                            egui::vec2(4.0, rects[3].height()),
                        )
                    } else {
                        egui::Rect::from_min_max(egui::pos2(screen.right() - 4.0, top), screen.right_bottom())
                    }),
                ];
                let pointer = ui_root.ctx().pointer_latest_pos();
                let (pressed, down) = ui_root.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down()));
                let over = pointer.and_then(|p| {
                    sashes.iter().find(|(_, r)| r.expand(1.0).contains(p)).map(|(i, _)| *i)
                });
                if pressed {
                    *sash_drag = over;
                }
                if !down {
                    *sash_drag = None;
                }
                match (over, *sash_hover) {
                    (Some(i), Some((j, _))) if i == j => {}
                    (Some(i), _) => *sash_hover = Some((i, std::time::Instant::now())),
                    (None, _) => *sash_hover = None,
                }
                // Not while something else is being dragged across it.
                let rested = sash_hover
                    .filter(|(_, since)| !down && since.elapsed() >= SASH_DELAY)
                    .map(|(i, _)| i);
                if let Some((_, rect)) = sash_drag.or(rested).and_then(|lit| sashes.iter().find(|(i, _)| *i == lit)) {
                    ui_root.painter().rect_filled(*rect, egui::CornerRadius::same(2), accent);
                }
            }

            // Asked once, over everything: closing the window over an edited
            // script would throw the edit away. The backdrop or Escape is
            // Cancel, the same as the button.
            if confirm_exit {
                let modal = egui::Modal::new(egui::Id::new("confirm_exit")).show(
                    ui_root.ctx(),
                    |ui| {
                        ui.set_width(380.0);
                        ui.heading("Unsaved changes");
                        ui.label(match dirty_names.as_slice() {
                            [one] => format!("{one} has been edited since it was last saved."),
                            many => format!("{} have been edited since they were last saved.", many.join(", ")),
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let save = if dirty_names.len() > 1 { "Save all and quit" } else { "Save and quit" };
                            if ui.add(primary(ui_theme, icons::codicon::SAVE, save)).clicked() {
                                save_and_quit = true;
                            }
                            if ui.button("Quit without saving").clicked() {
                                quit_now = true;
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_exit = true;
                            }
                        });
                    },
                );
                if modal.should_close() {
                    cancel_exit = true;
                }
            }

            // A right click's Delete: to the Trash, from where it can be put
            // back, once asked.
            if let Some(path) = &trash_asked {
                let modal = egui::Modal::new(egui::Id::new("confirm_trash")).show(ui_root.ctx(), |ui| {
                    ui.set_width(380.0);
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    ui.heading(format!("Delete {name}?"));
                    ui.label(if path.is_dir() {
                        format!("{} and everything in it go to the Trash, from where they can be put back.", path.display())
                    } else {
                        format!("{} goes to the Trash, from where it can be put back.", path.display())
                    });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.add(primary(ui_theme, icons::codicon::TRASH, "Move to the Trash")).clicked() {
                            trash_now = true;
                        }
                        if ui.button("Cancel").clicked() {
                            trash_cancel = true;
                        }
                    });
                });
                if modal.should_close() {
                    trash_cancel = true;
                }
            }

            // A cross on an edited file in the editor's column.
            if let Some(path) = &close_asked {
                let modal = egui::Modal::new(egui::Id::new("confirm_close")).show(ui_root.ctx(), |ui| {
                    ui.set_width(380.0);
                    ui.heading("Unsaved changes");
                    let name = if path.is_empty() { "untitled" } else { path.as_str() };
                    ui.label(format!("{name} has been edited since it was last saved. Save it before it is closed?"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if !path.is_empty() && ui.add(primary(ui_theme, icons::codicon::SAVE, "Save and close")).clicked() {
                            close_saving = true;
                        }
                        if ui.button("Close without saving").clicked() {
                            close_anyway = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_cancel = true;
                        }
                    });
                });
                if modal.should_close() {
                    close_cancel = true;
                }
            }
        });

        self.viewport_size = wanted;
        self.viewport_rect = vp_rect;
        self.panels = rects;
        self.float_sizes = out_sizes;
        self.resizing = out_resizing;
        self.docked_open = out_open;
        // Written back from the panels: folded means all three are, so
        // dragging one out clears it and `N` then folds everything again.
        let folded = DOCKED.iter().all(|&i| !self.docked_open[i]);
        app_config.panels_folded = folded;
        self.last_folded = folded;
        let each = self.docked_open.map(|open| !open);
        app_config.toolbar_folded = each[0];
        app_config.log_folded = each[1];
        app_config.simulation_folded = each[3];
        self.last_each = each;
        self.run_request |= run_request;
        self.restart_request |= restart_request;
        shared.reset_requested |= reset_request;
        // Back to nothing: the welcome comes back with the empty scene.
        if reset_request {
            self.welcome_gone = false;
            self.welcome_camera = None;
            self.welcome_settle = 3;
        }
        self.central_tab = central_tab.get();
        // Running something is for watching it: back to the scene.
        if run_request || restart_request || launch_request {
            self.central_tab = CentralTab::Renderer;
        }
        // Followed from the documentation: a file opened in the editor, since
        // the middle was showing the page; a folder shown open in the tree.
        match docs_request {
            Some(docs::Request::Open(path)) => self.open_in_editor(&path),
            Some(docs::Request::Reveal(dir)) => {
                let mut open = std::path::PathBuf::new();
                for part in dir.components() {
                    open.push(part);
                    self.files.open.insert(open.clone());
                }
                self.side_tab = SideTab::Files;
                self.docked_open[3] = true;
            }
            None => {}
        }
        // A definition's file, from the editor's peek.
        if let Some(path) = editor_open {
            self.open_in_editor(&path);
        }
        // What a row of the scripts tab asked for.
        match tree_action {
            Some(TreeAction::Open(path) | TreeAction::Created(path)) => self.open_in_editor(&path),
            Some(TreeAction::Docs(path)) => match self.docs.open(&self.ctx, &path) {
                Ok(()) => self.central_tab = CentralTab::Docs,
                Err(e) => self.notes.push(e),
            },
            Some(TreeAction::Render(path)) => self.send_to_renderer(&path),
            Some(TreeAction::Trash(path)) => self.trash_asked = Some(path),
            Some(TreeAction::Renamed(from, to)) => self.renamed(&from, &to),
            None => {}
        }
        if trash_now {
            if let Some(path) = self.trash_asked.take() {
                // Whole, as the tree's paths are relative to where kalast
                // started, and not every platform's Trash reads them so.
                let whole = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                match trash::delete(&whole) {
                    Ok(()) => {
                        self.notes.push(format!("moved {} to the Trash", path.display()));
                        self.files.reread();
                        self.thrown_away(&path);
                    }
                    Err(e) => self.notes.push(format!("cannot move {} to the Trash: {e}", path.display())),
                }
            }
        }
        if trash_cancel {
            self.trash_asked = None;
        }
        // The side panel's open button: the system's dialog, any file. A
        // mesh too big to edit goes to the scene instead.
        if open_external {
            if let Some(path) = rfd::FileDialog::new().set_title("Open a file").pick_file() {
                let path = tree_path(&path);
                let big_mesh = path.extension().is_some_and(|e| e == "obj") && read_text(&path).is_err();
                if big_mesh {
                    self.send_to_renderer(&path);
                } else {
                    self.open_in_editor(&path);
                }
            }
        }
        // The editor's column.
        match nav_click {
            Some(NavClick::Show(path)) => self.open_in_editor(std::path::Path::new(&path)),
            Some(NavClick::Close(path)) => {
                let edited = if path == self.script_path.trim() {
                    self.script_dirty
                } else {
                    self.stash.get(&path).is_some_and(|(_, dirty)| *dirty)
                };
                if edited {
                    self.close_asked = Some(path);
                } else {
                    self.close_file(&path);
                }
            }
            Some(NavClick::Line(line)) => self.script_editor.go_to_line(&self.ctx, &self.script, line),
            None => {}
        }
        if close_saving || close_anyway {
            if let Some(path) = self.close_asked.take() {
                if close_saving {
                    let text = if path == self.script_path.trim() {
                        Some(self.script.clone())
                    } else {
                        self.stash.get(&path).map(|(text, _)| text.clone())
                    };
                    match text.map(|t| std::fs::write(&path, t)) {
                        Some(Ok(())) => self.notes.push(format!("saved {path}")),
                        Some(Err(e)) => self.notes.push(format!("cannot save {path}: {e}")),
                        None => {}
                    }
                }
                self.close_file(&path);
            }
        }
        if close_cancel {
            self.close_asked = None;
        }
        // The shown file's buttons.
        if render_now {
            let path = std::path::PathBuf::from(self.script_path.trim());
            self.send_to_renderer(&path);
        }
        if preview_now {
            let path = std::path::PathBuf::from(self.script_path.trim());
            match self.docs.open(&self.ctx, &path) {
                Ok(()) => self.central_tab = CentralTab::Docs,
                Err(e) => self.notes.push(e),
            }
        }
        // Changed in the app tab: remembered. See `settings`.
        if remember && !cfg!(test) {
            crate::app::settings::save(&crate::app::settings::Remembered::of(app_config));
        }
        if save_and_quit {
            self.save_stashed();
            self.save_request = true;
            self.exit_after_save = true;
        }
        self.quit_request |= quit_now;
        if save_and_quit || quit_now || cancel_exit {
            self.confirm_exit = false;
        }
        self.save_request |= save_request || editor_save;
        // What the editor had to say -- a language server started, Neovim
        // missing -- in the kalast tab.
        for line in self.notes.drain(..) {
            shared.kalast_log.push(line);
        }
        for line in self.script_editor.log.drain(..) {
            shared.kalast_log.push(line);
        }
        self.update_request |= update_request;
        self.relaunch_request |= relaunch_request;
        self.build_request |= build_request;
        self.launch_request |= launch_request;
        self.state
            .handle_platform_output(window, output.platform_output);

        let jobs = self
            .ctx
            .tessellate(output.shapes, output.pixels_per_point);
        let desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [surface_size.0, surface_size.1],
            pixels_per_point: output.pixels_per_point,
        };

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("egui") });
        for (id, deltas) in &output.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let extra = self
            .renderer
            .update_buffers(device, queue, &mut encoder, &jobs, &desc);

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui"),
                timestamp_writes: timestamps,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.renderer.render(&mut pass.forget_lifetime(), &jobs, &desc);
        }

        queue.submit(extra.into_iter().chain([encoder.finish()]));
        for id in &output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        // Both lists are iterated by reference, which leaves them full, and
        // epaint asserts in debug builds that a delta it handed out was
        // consumed -- so a debug editor panicked on its first frame, on a
        // path a release build never checks.
        output.textures_delta.clear();

        self.viewport_size
    }

    /// Whether a pointer event belongs to the scene rather than the UI.
    ///
    /// True when the pointer is over the viewport and egui is not in the
    /// middle of a drag of its own -- a slider grabbed and dragged across the
    /// viewport keeps belonging to the slider.
    pub fn pointer_on_scene(&self) -> bool {
        if self.ctx.egui_is_using_pointer() {
            return false;
        }
        let Some(p) = self.ctx.pointer_latest_pos() else {
            return false;
        };
        if !self.viewport_rect.contains(p) {
            return false;
        }
        // And not over a panel drawn on top of it. In focus mode the viewport
        // *is* the whole window, so the rect test alone put every panel on the
        // scene's side -- and scrolling the script zoomed the render, which is
        // the thing the docked layout had already been fixed not to do.
        !self.panels.iter().any(|r| r.contains(p))
    }

    /// Give a window event to the UI first.
    ///
    /// Returns true when egui wants it -- a click on a slider, a keystroke in
    /// the script editor -- in which case the camera controller must not also
    /// act on it, or dragging a slider would orbit the scene behind it.
    /// Points per physical pixel, for turning a cursor position into the
    /// coordinates the panel rectangles are in.
    pub fn scale(&self) -> f32 {
        self.ctx.pixels_per_point()
    }

    pub fn on_window_event(
        &mut self,
        window: &winit::window::Window,
        event: &winit::event::WindowEvent,
    ) -> bool {
        self.state.on_window_event(window, event).consumed
    }
}

#[cfg(test)]
mod reveal_tests {
    use super::*;

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1500.0, 925.0))
    }

    fn nothing() -> [egui::Rect; 4] {
        [egui::Rect::NOTHING; 4]
    }

    #[test]
    fn every_panel_shows_when_the_renderer_does_not_own_the_window() {
        let at = Some(egui::pos2(750.0, 500.0));
        assert_eq!(reveal_panels(false, at, screen(), &nothing()), [true; 4]);
        // Even with no pointer at all.
        assert_eq!(reveal_panels(false, None, screen(), &nothing()), [true; 4]);
    }

    #[test]
    fn each_edge_summons_its_own_panel_and_no_other() {
        let s = screen();
        let cases = [
            (egui::pos2(750.0, 4.0), [true, false, false, false]),
            (egui::pos2(750.0, 921.0), [false, true, false, false]),
            (egui::pos2(4.0, 500.0), [false, false, true, false]),
            (egui::pos2(1496.0, 500.0), [false, false, false, true]),
        ];
        for (p, want) in cases {
            assert_eq!(
                reveal_panels(true, Some(p), s, &nothing()),
                want,
                "pointer at {p:?}"
            );
        }
    }

    #[test]
    fn the_middle_summons_nothing() {
        let at = Some(egui::pos2(750.0, 500.0));
        assert_eq!(reveal_panels(true, at, screen(), &nothing()), [false; 4]);
    }

    /// The 24-point strip is far narrower than a panel, so reaching for
    /// anything inside one would dismiss it if only the strip counted.
    #[test]
    fn a_panel_stays_while_the_pointer_is_over_it() {
        let s = screen();
        let mut panels = nothing();
        // The config panel as drawn: 240 wide, down the right-hand side.
        panels[3] = egui::Rect::from_min_max(egui::pos2(1260.0, 0.0), egui::pos2(1500.0, 925.0));

        // Well inside it, and nowhere near the edge strip.
        let deep = egui::pos2(1300.0, 500.0);
        assert!(deep.x < s.right() - EDGE, "the test point must clear the strip");
        assert_eq!(
            reveal_panels(true, Some(deep), s, &panels),
            [false, false, false, true],
            "a panel must stay while the pointer is on it"
        );
    }

    #[test]
    fn no_pointer_shows_nothing_in_focus_mode() {
        assert_eq!(reveal_panels(true, None, screen(), &nothing()), [false; 4]);
    }
}

/// Everything written to stdout and stderr, mirrored into the log panel.
///
/// The panel used to be fed by teeing Python's `sys.stdout`, which caught
/// `print` and tracebacks and nothing else. The renderer's own output --
/// `H` printing the camera, the mesh loader, every `debug_*` flag -- is
/// `println!` from Rust, straight to file descriptor 1, and never went near
/// Python. Pressing `H` and seeing nothing in the log is what that looks
/// like.
///
/// So it is captured a level down, where both end up: the descriptors are
/// pointed at a pipe, and a thread of its own reads it, writes it on to the
/// real stdout -- so a terminal still shows everything it did -- and keeps
/// the lines for the next frame to move into the log.
///
/// A thread, not the frame, because a pipe holds 64 KB. Emptied only by the
/// frame, a script that printed more than that between two frames -- or
/// before the first one, which is when a script named on the command line
/// runs -- blocked in `write` with nothing left to read it, and hung.
pub struct StdioCapture {
    /// The original stdout, kept so output still reaches the terminal.
    tty: std::fs::File,
    /// Whole lines the reader has taken off the pipe, stamped then, waiting
    /// for a frame.
    lines: std::sync::Arc<std::sync::Mutex<Vec<Entry>>>,
    /// Signalled by the reader once the pipe has closed and everything in
    /// it has gone to the terminal.
    #[cfg_attr(not(any(unix, windows)), allow(dead_code))]
    done: std::sync::mpsc::Receiver<()>,
    /// What `restore` puts back on Windows.
    #[cfg(windows)]
    saved: WindowsHandles,
}

/// The standard handles a Windows capture replaced, and the pipe it put in
/// their place. As integers, not `HANDLE`s: a raw pointer would make the
/// capture `!Send` for no reason.
#[cfg(windows)]
struct WindowsHandles {
    out: isize,
    err: isize,
    /// The pipe's write end. `SetStdHandle` does not keep a handle open, so
    /// this is what does -- dropped, stdout would name a closed handle -- and
    /// closing it in `restore` is what lets the reader see the pipe end.
    writer: Option<std::os::windows::io::OwnedHandle>,
    /// Whether the capture cleared the shell's `STARTF_HASSHELLDATA`, for
    /// `restore` to set it again.
    shell_data: bool,
}

impl StdioCapture {
    /// The terminal this process started with, for a child to write to.
    ///
    /// A child spawned with inherited stdio writes into the *pipe* this holds
    /// open, which is fine while the editor is here to read it -- and fatal
    /// the moment the editor exits, because the read end goes with it and the
    /// child's next `write` takes a `SIGPIPE`. A launched example outlives
    /// the editor that launched it, so it gets the terminal instead.
    pub fn terminal(&self) -> Option<std::process::Stdio> {
        self.tty.try_clone().ok().map(std::process::Stdio::from)
    }

    /// Redirect stdout and stderr into a pipe. `None` if that fails, in which
    /// case output keeps going to the terminal and the panel stays empty --
    /// worth nobody's run failing over.
    #[cfg(not(any(unix, windows)))]
    pub fn new() -> Option<Self> {
        None
    }

    /// Whether a capture holds stdout in this process now -- this app's, or
    /// another's -- as against none having been set up.
    #[cfg(any(unix, windows))]
    pub fn active() -> bool {
        CAPTURING.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// `None` while another capture is active: see `CAPTURING`.
    #[cfg(any(unix, windows))]
    pub fn new() -> Option<Self> {
        use std::sync::atomic::Ordering;

        if CAPTURING.swap(true, Ordering::SeqCst) {
            return None;
        }
        let capture = Self::redirect();
        if capture.is_none() {
            CAPTURING.store(false, Ordering::SeqCst);
        }
        capture
    }

    #[cfg(unix)]
    fn redirect() -> Option<Self> {
        use std::os::fd::{AsRawFd as _, FromRawFd as _};

        let (reader, writer) = std::io::pipe().ok()?;
        // SAFETY: `dup` copies the current stdout so it can be written to
        // afterwards; a negative return means nothing was opened.
        let saved = unsafe { libc::dup(libc::STDOUT_FILENO) };
        if saved < 0 {
            return None;
        }
        // SAFETY: `saved` was just opened here and nothing else owns it.
        let tty = unsafe { std::fs::File::from_raw_fd(saved) };
        let tee = tty.try_clone().ok()?;
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (finished, done) = std::sync::mpsc::channel();

        // Started before the redirect, so a failure leaves the descriptors
        // as they were; the reader then sees the pipe close and returns.
        let kept = lines.clone();
        std::thread::Builder::new()
            .name("kalast-stdio".into())
            .spawn(move || pump(reader, Terminal::new(tee), &kept, finished))
            .ok()?;

        // SAFETY: plain descriptor calls pointing 1 and 2 at the pipe. A
        // negative return means that redirect did not happen; stdout is put
        // back in case it was the second that failed.
        unsafe {
            if libc::dup2(writer.as_raw_fd(), libc::STDOUT_FILENO) < 0
                || libc::dup2(writer.as_raw_fd(), libc::STDERR_FILENO) < 0
            {
                libc::dup2(tty.as_raw_fd(), libc::STDOUT_FILENO);
                return None;
            }
        }
        script_terminal(tty.try_clone().ok());
        engine_out(writer.try_clone().ok().map(|w| std::fs::File::from(std::os::fd::OwnedFd::from(w))));

        Some(Self { tty, lines, done })
    }

    /// Windows has no descriptor 1 to `dup2` over, but it has the process's
    /// standard handles, and those are what the engine writes to: `println!`
    /// asks `GetStdHandle` on every write (`std/src/sys/stdio/windows.rs`),
    /// and a child spawned with inherited output is handed them at spawn. So
    /// the pipe goes in with `SetStdHandle`, and what the engine and cargo
    /// print reaches the kalast tab as it does on macOS and Linux. This used
    /// to be `None` on the belief that `println!` held its handle; it asks
    /// each time.
    ///
    /// Not redirected: the C runtime's own descriptors 1 and 2, set up from
    /// the handles the process started with. Only C code calling `printf`
    /// writes there, which nothing kalast runs does -- Python's output has
    /// its own route, `capture_output`. Moving them would mean `_dup2` on
    /// descriptors a double-clicked executable does not have, and the MSVC
    /// runtime answers an invalid descriptor by ending the process unless its
    /// invalid-parameter handler is replaced first.
    ///
    /// Double-clicked there is no terminal, and stdout reads as NULL -- the
    /// shell's monitor is in its slot (`STARTF_HASSHELLDATA`) -- so the
    /// terminal's copy goes to `NUL` and the lines still reach the log, the
    /// only place anyone is reading them.
    #[cfg(windows)]
    fn redirect() -> Option<Self> {
        use std::os::windows::io::{AsRawHandle as _, BorrowedHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Console::{GetStdHandle, SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};

        let (reader, writer) = std::io::pipe().ok()?;
        // SAFETY: plain queries of this process's standard handles.
        let (out, err) = unsafe { (GetStdHandle(STD_OUTPUT_HANDLE), GetStdHandle(STD_ERROR_HANDLE)) };
        let tty = if out.is_null() || out == INVALID_HANDLE_VALUE {
            std::fs::OpenOptions::new().write(true).open("NUL").ok()?
        } else {
            // SAFETY: `out` is this process's stdout, open for as long as the
            // process is. It is duplicated here, not taken.
            std::fs::File::from(unsafe { BorrowedHandle::borrow_raw(out) }.try_clone_to_owned().ok()?)
        };
        let tee = tty.try_clone().ok()?;
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (finished, done) = std::sync::mpsc::channel();

        // As on Unix: started first, so a failure below leaves stdout alone
        // and the reader sees the pipe close.
        let kept = lines.clone();
        std::thread::Builder::new()
            .name("kalast-stdio".into())
            .spawn(move || pump(reader, Terminal::new(tee), &kept, finished))
            .ok()?;

        let writer = OwnedHandle::from(writer);
        // The slot is about to hold a handle, so the shell's word that it
        // holds a monitor stops being true here. Without it the slot reads
        // as what it holds, the monitor, which is what `restore` puts back.
        let shell_data = take_shell_data();
        // SAFETY: a plain query, as above.
        let out = if shell_data { unsafe { GetStdHandle(STD_OUTPUT_HANDLE) } } else { out };
        // SAFETY: both standard handles pointed at the pipe, which `writer`
        // keeps open until `restore` has put these two back. A failure puts
        // them back at once.
        unsafe {
            if SetStdHandle(STD_OUTPUT_HANDLE, writer.as_raw_handle()) == 0
                || SetStdHandle(STD_ERROR_HANDLE, writer.as_raw_handle()) == 0
            {
                SetStdHandle(STD_OUTPUT_HANDLE, out);
                SetStdHandle(STD_ERROR_HANDLE, err);
                if shell_data {
                    give_back_shell_data();
                }
                return None;
            }
        }
        script_terminal(tty.try_clone().ok());
        engine_out(writer.try_clone().ok().map(std::fs::File::from));

        let saved = WindowsHandles { out: out as isize, err: err as isize, writer: Some(writer), shell_data };
        Some(Self { tty, lines, done, saved })
    }

    /// Give stdout and stderr back, flushing whatever is still in the pipe.
    ///
    /// Not optional: the tee back to the terminal happens in `drain`, so
    /// anything written after the last frame -- which includes everything a
    /// script prints on its way out -- would be swallowed with the pipe. A
    /// test that printed its result and stopped saw nothing at all.
    #[cfg(not(any(unix, windows)))]
    fn restore(&mut self) {
        // Unreachable: `new` returns `None` here, so no instance exists to
        // drop. Present so the type compiles.
    }

    #[cfg(windows)]
    fn restore(&mut self) {
        use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};

        // What `println!` still holds in its buffer belongs to the pipe.
        let _ = std::io::Write::flush(&mut std::io::stdout());
        // Handles first, so anything printed from here on goes where it went
        // before; then the write end closes, and the pipe with it once no
        // child holds a copy.
        // The shell's flag before its monitor, so stdout never reads as a
        // monitor in between.
        if std::mem::take(&mut self.saved.shell_data) {
            give_back_shell_data();
        }
        // SAFETY: putting back the handles saved in `redirect`.
        unsafe {
            SetStdHandle(STD_OUTPUT_HANDLE, self.saved.out as _);
            SetStdHandle(STD_ERROR_HANDLE, self.saved.err as _);
        }
        self.saved.writer = None;
        engine_out(None);
        script_terminal(None);

        // Bounded, as on Unix: a child that inherited the pipe holds it open.
        let _ = self.done.recv_timeout(std::time::Duration::from_millis(500));
        CAPTURING.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(unix)]
    fn restore(&mut self) {
        use std::os::fd::AsRawFd as _;

        // What `println!` still holds in its buffer belongs to the pipe.
        let _ = std::io::Write::flush(&mut std::io::stdout());
        // Descriptors first, so anything printed from here on goes straight
        // out, and the pipe closes once nothing else holds it.
        // SAFETY: putting back the descriptor saved in `new`.
        unsafe {
            libc::dup2(self.tty.as_raw_fd(), libc::STDOUT_FILENO);
            libc::dup2(self.tty.as_raw_fd(), libc::STDERR_FILENO);
        }
        engine_out(None);
        script_terminal(None);

        // Then give the reader the time to put the rest on the terminal --
        // the last thing a script printed, the line it exists to report.
        // Bounded: a child that inherited the pipe, a cargo build still
        // running, holds it open, and closing the window must not wait on it.
        let _ = self.done.recv_timeout(std::time::Duration::from_millis(500));
        CAPTURING.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Move the lines written since last time into the log.
    pub fn drain(&mut self, log: &mut Log) {
        let lines = std::mem::take(&mut *self.lines.lock().unwrap_or_else(|e| e.into_inner()));
        for line in lines {
            log.push_at(line.time, line.text);
        }
    }
}

/// A program the Windows shell starts -- a double-click, the Start menu, the
/// taskbar -- is told which monitor to open on through its stdout slot, with
/// this flag in its start-up flags to say the slot holds a monitor and not a
/// handle. While it is set, `GetStdHandle(STD_OUTPUT_HANDLE)` answers NULL
/// whatever the slot holds: kernelbase tests the flag before reading the
/// slot, and `SetStdHandle` writes the slot and leaves the flag. So in a
/// double-clicked kalast.exe the capture's pipe went in and never came back
/// out, and `println!`, which asks `GetStdHandle` on every write, dropped
/// each line in silence. stderr has no such flag.
///
/// Not an overlay taking stdout, which is what it looked like for two days
/// (`notes/TIMELINE.md`, 26 September), nor anything writing to stdout at
/// all: read from outside, the slot held the pipe in 3.4 million reads of
/// 3.4 million while `GetStdHandle` inside answered NULL. The user's kalast
/// had started with flags 0xC01; the tests, started from a terminal or
/// through `explorer.exe` on a shortcut, with 0x801, and never failed. A
/// probe started with the flag and without it showed the flag alone decides.
///
/// A capture clears it while it holds stdout, and `restore` sets it again
/// with the monitor back in the slot. The window still opens on the monitor
/// the shell chose -- measured with the flag cleared at the top of `main`,
/// before any window existed: Windows has taken it by then.
#[cfg(windows)]
const STARTF_HASSHELLDATA: u32 = 0x400;

/// This process's start-up flags, the `dwFlags` it was started with, where
/// `GetStdHandle` looks for `STARTF_HASSHELLDATA`. `None` if they cannot be
/// reached, which leaves everything as it was.
#[cfg(windows)]
fn window_flags() -> Option<&'static std::sync::atomic::AtomicU32> {
    use windows_sys::Wdk::System::Threading::{NtQueryInformationProcess, ProcessBasicInformation};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, PROCESS_BASIC_INFORMATION};

    // `WindowFlags`, past the part of RTL_USER_PROCESS_PARAMETERS that
    // winternl.h names and where it has been since NT 4. 0xA4 is the offset
    // kernelbase's `GetStdHandle` tests.
    #[cfg(target_pointer_width = "64")]
    const WINDOW_FLAGS: usize = 0xA4;
    #[cfg(target_pointer_width = "32")]
    const WINDOW_FLAGS: usize = 0x68;

    // SAFETY: plain data, for the query to fill.
    let mut info: PROCESS_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a query about this process, into a buffer of the size given.
    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            ProcessBasicInformation,
            (&raw mut info).cast(),
            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
            std::ptr::null_mut(),
        )
    };
    if status < 0 || info.PebBaseAddress.is_null() {
        return None;
    }
    // SAFETY: this process's PEB and its parameters, which live as long as
    // the process; the flags are a 4-aligned `u32`, as the atomic needs.
    unsafe {
        let params = (*info.PebBaseAddress).ProcessParameters;
        if params.is_null() {
            return None;
        }
        Some(std::sync::atomic::AtomicU32::from_ptr(params.cast::<u8>().add(WINDOW_FLAGS).cast()))
    }
}

/// Clear `STARTF_HASSHELLDATA`, saying whether it was set.
#[cfg(windows)]
fn take_shell_data() -> bool {
    use std::sync::atomic::Ordering;
    window_flags().is_some_and(|flags| flags.fetch_and(!STARTF_HASSHELLDATA, Ordering::SeqCst) & STARTF_HASSHELLDATA != 0)
}

/// Set `STARTF_HASSHELLDATA`, as the shell left it.
#[cfg(windows)]
fn give_back_shell_data() {
    if let Some(flags) = window_flags() {
        flags.fetch_or(STARTF_HASSHELLDATA, std::sync::atomic::Ordering::SeqCst);
    }
}

/// One capture per process: stdout and stderr are the process's. A second one
/// saved the first one's pipe as "the terminal", and released out of order
/// left stdout on a pipe nobody read. Seen in the test binary, where several
/// tests start the UI app side by side: output stopped mid-run, or the test
/// harness died on a broken pipe.
#[cfg(any(unix, windows))]
static CAPTURING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A copy of the capture's pipe, for kalast's own output while one runs.
static ENGINE_OUT: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);

#[cfg_attr(not(any(unix, windows)), allow(dead_code))]
fn engine_out(pipe: Option<std::fs::File>) {
    *ENGINE_OUT.lock().unwrap_or_else(|e| e.into_inner()) = pipe;
}

/// kalast's `println!` and `eprintln!` (`src/lib.rs`): into the capture's
/// pipe while one runs -- the kalast tab, and on to the terminal through the
/// reader -- else as std prints them.
///
/// Not through stdout, because stdout is the process's, not kalast's, and
/// on Windows it has gone missing under kalast twice: in a double-clicked
/// kalast.exe it read as NULL for the whole session (`STARTF_HASSHELLDATA`),
/// and a C runtime mirroring its descriptors made a script's file stdout
/// once the script closed descriptor 1 (`gui_c_runtime` in
/// `src/bin/kalast.rs`). Nothing else holds this copy of the pipe. A hosted
/// Rust example is a crate of its own with no capture in it, so it hands its
/// lines to the host (`HostApi::print`).
#[doc(hidden)]
pub fn engine_write(args: std::fmt::Arguments<'_>, err: bool) {
    use std::io::Write as _;

    // Formatted first and written once: `write_fmt` goes piece by piece,
    // and another writer on the pipe could cut in between two pieces.
    let text = args.to_string();
    if let Some(pipe) = ENGINE_OUT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        let _ = pipe.write_all(text.as_bytes());
        return;
    }
    if crate::app::hosted::print_to_host(&text) {
        return;
    }
    // std's own macros, so a test harness still captures it.
    if err {
        ::std::eprint!("{text}");
    } else {
        ::std::print!("{text}");
    }
}

/// Stdout and stderr for a child whose output belongs in the kalast tab --
/// cargo building a Rust example -- given the pipe itself rather than
/// whatever stdout is at the moment it starts. `None` without a capture:
/// the child then inherits, as before.
pub fn engine_stdio() -> Option<(std::process::Stdio, std::process::Stdio)> {
    let out = ENGINE_OUT.lock().unwrap_or_else(|e| e.into_inner());
    let pipe = out.as_ref()?;
    Some((pipe.try_clone().ok()?.into(), pipe.try_clone().ok()?.into()))
}

/// What a script writes through Python's `sys.stdout` and `sys.stderr`, for
/// the log's script tab.
///
/// Apart from `StdioCapture`, which cannot tell a script's `print` from the
/// engine's `println!`: both are bytes on descriptor 1 by the time the pipe
/// has them. A level up they are not -- a script's output passes through
/// `sys.stdout` first -- so the UI app swaps in a writer that lands here
/// (`kalast.editor.capture_output`), and the pipe keeps the rest: the
/// engine, the update check, cargo, C libraries.
static SCRIPT_OUTPUT: std::sync::Mutex<ScriptOutput> =
    std::sync::Mutex::new(ScriptOutput { partial: Vec::new(), lines: Vec::new(), terminal: None });

struct ScriptOutput {
    partial: Vec<u8>,
    lines: Vec<Entry>,
    /// The terminal, while a capture holds stdout. The script's copy goes
    /// here and not to descriptor 1, which is the capture's pipe and would
    /// put the same lines in the kalast tab as well.
    terminal: Option<Terminal>,
}

/// Where a capture's copies go: the stdout the process had before it.
///
/// The bytes as they are, except to a Windows console whose code page is
/// not UTF-8: there, as `println!` writes one, UTF-16 through
/// `WriteConsoleW`. Bytes through `WriteFile` come out in the code page and
/// garble anything that is not ASCII -- a path with an accent in it. A read
/// can end inside a character, so the start of a cut one waits for the rest.
struct Terminal {
    file: std::fs::File,
    #[cfg(windows)]
    pending: Vec<u8>,
}

impl Terminal {
    fn new(file: std::fs::File) -> Self {
        Self {
            file,
            #[cfg(windows)]
            pending: Vec::new(),
        }
    }

    /// Best effort: a terminal that has gone away is no reason to fail.
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write as _;

        #[cfg(windows)]
        if console_wants_utf16(&self.file) {
            return self.write_utf16(bytes);
        }
        let _ = self.file.write_all(bytes);
        let _ = self.file.flush();
    }

    #[cfg(windows)]
    fn write_utf16(&mut self, bytes: &[u8]) {
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::System::Console::WriteConsoleW;

        self.pending.extend_from_slice(bytes);
        let whole = whole_utf8(&self.pending);
        let wide: Vec<u16> = String::from_utf8_lossy(&self.pending[..whole]).encode_utf16().collect();
        self.pending.drain(..whole);
        let mut rest = &wide[..];
        while !rest.is_empty() {
            // It may take fewer units than it is given, and a surrogate pair
            // is not split across two calls.
            let mut n = rest.len().min(8192);
            if n < rest.len() && (0xD800..0xDC00).contains(&rest[n - 1]) {
                n -= 1;
            }
            let mut written = 0u32;
            // SAFETY: `rest` holds at least `n` units; the handle is the
            // console this terminal writes to.
            let ok = unsafe {
                WriteConsoleW(self.file.as_raw_handle(), rest.as_ptr(), n as u32, &mut written, std::ptr::null())
            };
            if ok == 0 || written == 0 {
                break;
            }
            rest = &rest[written as usize..];
        }
    }
}

/// Whether `file` is a console that takes UTF-16: the test `println!` makes.
#[cfg(windows)]
fn console_wants_utf16(file: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::System::Console::{GetConsoleMode, GetConsoleOutputCP};

    const CP_UTF8: u32 = 65001;
    let mut mode = 0;
    // SAFETY: queries on a handle `file` owns.
    unsafe { GetConsoleMode(file.as_raw_handle(), &mut mode) != 0 && GetConsoleOutputCP() != CP_UTF8 }
}

/// How much of `bytes` is whole UTF-8: all of it, or up to a character cut
/// off at the end. An invalid byte counts as whole -- it prints as U+FFFD --
/// so nothing waits forever.
#[cfg(any(windows, test))]
fn whole_utf8(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        _ => bytes.len(),
    }
}

/// Write a script's output: on to the terminal, and into the script tab once
/// a line is whole.
pub fn script_write(text: &str) {
    use std::io::Write as _;

    let mut out = SCRIPT_OUTPUT.lock().unwrap_or_else(|e| e.into_inner());
    match out.terminal.as_mut() {
        Some(terminal) => terminal.write(text.as_bytes()),
        None => {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(text.as_bytes());
            let _ = stdout.flush();
        }
    }
    out.partial.extend_from_slice(text.as_bytes());
    // Stamped here, when the script wrote it, not when a frame moves it: a
    // script named on the command line prints before there is a frame.
    let mut time = None;
    while let Some(i) = out.partial.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = out.partial.drain(..=i).collect();
        out.lines.push(Entry {
            time: time.get_or_insert_with(|| crate::app::clock::now().time()).clone(),
            text: String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_string(),
        });
    }
}

/// Move the lines a script has written since last time into its log.
pub fn drain_script_output(log: &mut Log) {
    let lines = std::mem::take(&mut SCRIPT_OUTPUT.lock().unwrap_or_else(|e| e.into_inner()).lines);
    for line in lines {
        log.push_at(line.time, line.text);
    }
}

/// The log's python tab: the lines typed at it, waiting for the loop to run
/// them between frames, and what running them printed.
///
/// Out here rather than on the app, as the script's output is: a line runs
/// with the app borrowed by whatever it calls, and what it prints has to land
/// where that borrow does not reach.
static CONSOLE: std::sync::Mutex<Console> = std::sync::Mutex::new(Console {
    input: std::collections::VecDeque::new(),
    complete: None,
    offer: None,
    partial: Vec::new(),
    lines: Vec::new(),
    greeting: Vec::new(),
    more: false,
    interrupt: false,
});

struct Console {
    input: std::collections::VecDeque<String>,
    /// The line as it stood when Tab was pressed, for Python to complete:
    /// only it knows the namespace.
    complete: Option<String>,
    /// Its answer: the line asked about, the part of it before the word being
    /// completed, and the completions of that word.
    offer: Option<(String, String, Vec<String>)>,
    partial: Vec<u8>,
    lines: Vec<Entry>,
    /// The banner's lines, kept apart from `lines` to go in as seen.
    greeting: Vec<Entry>,
    /// The last line opened a block -- a `for`, a `def` -- so the next is
    /// typed at `...`.
    more: bool,
    /// Ctrl+C at the prompt: Python drops the block it was collecting.
    interrupt: bool,
}

/// The most of one line the python tab keeps, in bytes.
const CONSOLE_LINE: usize = 4096;

fn console() -> std::sync::MutexGuard<'static, Console> {
    CONSOLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A line typed at the python tab, for the loop to run between frames.
pub fn console_submit(line: String) {
    console().input.push_back(line);
}

/// The next line typed, if any: `EditorTick::Console`, or a script's own
/// `step()` through `kalast.editor.serve_console`.
pub fn console_take() -> Option<String> {
    console().input.pop_front()
}

/// Whether the console has something for Python: a line, or a Tab.
pub fn console_pending() -> bool {
    let c = console();
    !c.input.is_empty() || c.complete.is_some() || c.interrupt
}

/// Ctrl+C at the prompt, as a terminal's: the line typed so far goes, and
/// so does any block Python was collecting, which only Python can drop.
pub fn console_interrupt() {
    console().interrupt = true;
    console_set_more(false);
}

/// Whether Ctrl+C was pressed since Python last looked.
pub fn console_take_interrupt() -> bool {
    std::mem::take(&mut console().interrupt)
}

/// Ask for completions of `line`, as Tab does. One at a time: a second Tab
/// before the answer replaces the question.
pub fn console_ask_completion(line: String) {
    console().complete = Some(line);
}

/// The line Tab asked about, if it has not been answered.
pub fn console_take_completion() -> Option<String> {
    console().complete.take()
}

/// Python's answer to a Tab: see `Console::offer`.
pub fn console_offer(line: String, head: String, matches: Vec<String>) {
    console().offer = Some((line, head, matches));
}

fn console_take_offer() -> Option<(String, String, Vec<String>)> {
    console().offer.take()
}

/// The python tab's greeting, `python`'s in a terminal: its version, then
/// where help is. Once, whoever asks first -- the `kalast` binary as it
/// starts, or Python itself when kalast runs under it. No news, so it puts
/// no dot on the tab: it did at every start.
pub fn console_greet(version: &str, platform: &str) {
    static GREETED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !GREETED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let time = crate::app::clock::now().time();
        let banner = format!(
            "Python {version} on {platform}\n\
             Type \"help\", \"copyright\", \"credits\" or \"license\" for more information."
        );
        // `lines`, not one line: some builds' `sys.version` has a newline.
        console()
            .greeting
            .extend(banner.lines().map(|text| Entry { time: time.clone(), text: text.to_string() }));
    }
}

/// What running a line printed, into the python tab once a line is whole.
pub fn console_write(text: &str) {
    let mut c = console();
    c.partial.extend_from_slice(text.as_bytes());
    while let Some(i) = c.partial.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = c.partial.drain(..=i).collect();
        let mut text = String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_string();
        // A line that long is a dump, not something to read, and laying it
        // out every frame stalled the window: its start, and how much is left.
        if text.len() > CONSOLE_LINE {
            let cut = (0..=CONSOLE_LINE).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
            let rest = text.len() - cut;
            text.truncate(cut);
            text.push_str(&format!(" ... ({rest} more characters)"));
        }
        c.lines.push(Entry { time: crate::app::clock::now().time(), text });
    }
}

/// Whether the console waits for the rest of a block.
pub fn console_set_more(more: bool) {
    console().more = more;
}

fn console_more() -> bool {
    console().more
}

/// Move what the console printed since last time into its log.
pub fn drain_console_output(log: &mut Log) {
    let (greeting, lines) = {
        let mut c = console();
        (std::mem::take(&mut c.greeting), std::mem::take(&mut c.lines))
    };
    for line in greeting {
        log.push_seen_at(line.time, line.text);
    }
    for line in lines {
        log.push_at(line.time, line.text);
    }
}

/// A log tab's lines. The kalast and script tabs put each after its time,
/// dimmed, in one piece so a copied line keeps its stamp. The python tab
/// reads as a terminal does: no stamps, the `>>>` in the prompt's colour and
/// the code after it highlighted, a traceback and the error it ends on in
/// red.
fn transcript(ui: &mut egui::Ui, log: &Log, stamped: bool, palette: &code::Palette) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let valign = ui.text_valign();
    let format = |color| egui::TextFormat { font_id: font.clone(), color, valign, ..Default::default() };
    let (stamp, text) = (format(ui.visuals().weak_text_color()), format(egui::Color32::PLACEHOLDER));
    let mut traceback = false;
    for entry in log.entries() {
        let mut line = egui::text::LayoutJob::default();
        if stamped {
            line.append(&entry.time, 0.0, stamp.clone());
            line.append(" ", 0.0, text.clone());
            line.append(&entry.text, 0.0, text.clone());
        } else if let Some((prompt, code)) = [">>> ", "... "]
            .iter()
            .find_map(|p| entry.text.strip_prefix(p).map(|code| (&p[..3], code)))
        {
            line.append(prompt, 0.0, format(palette.prompt));
            line.append(" ", 0.0, text.clone());
            code::append(&mut line, code, code::Lang::Python, palette, font.clone());
            traceback = false;
        } else {
            traceback |= entry.text.starts_with("Traceback (most recent call last):");
            // `NameError: ...`, `KeyboardInterrupt`: the line an error ends on.
            let error = is_error_line(&entry.text);
            let color = if traceback || error { palette.error } else { egui::Color32::PLACEHOLDER };
            line.append(&entry.text, 0.0, format(color));
            traceback &= !error;
        }
        ui.label(line);
    }
}

/// Whether a line is the one a Python error ends on: its name, and a message
/// after a colon or nothing -- `ZeroDivisionError: division by zero`,
/// `KeyboardInterrupt`.
fn is_error_line(line: &str) -> bool {
    let name = line.split(':').next().unwrap_or("");
    let last = name.rsplit('.').next().unwrap_or("");
    !name.contains(' ')
        && ["Error", "Exception", "Interrupt", "Exit", "Warning"].iter().any(|s| last.ends_with(s))
        && last.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// The python tab's input line: Enter hands it to the loop, `↑` and `↓` walk
/// back through what was typed before it, and Tab completes the word under
/// the cursor -- asked of Python between frames, so the answer comes a
/// frame or so later.
fn console_prompt(
    ui: &mut egui::Ui,
    input: &mut String,
    history: &mut Vec<String>,
    back: &mut usize,
    palette: &code::Palette,
) {
    let id = egui::Id::new("console_input");
    // Ctrl+C with nothing selected, as at a terminal's prompt: the line goes,
    // `KeyboardInterrupt` says so, and Python drops any block it was
    // collecting. With a selection it copies, as everywhere else.
    if ui.memory(|m| m.has_focus(id)) {
        let selected = egui::TextEdit::load_state(ui.ctx(), id)
            .and_then(|s| s.cursor.char_range())
            .is_some_and(|r| r.primary.index != r.secondary.index);
        let copy = !selected
            && ui.input_mut(|i| {
                let n = i.events.len();
                i.events.retain(|e| !matches!(e, egui::Event::Copy));
                i.events.len() != n
            });
        if copy {
            console_write(&format!("{} {input}\nKeyboardInterrupt\n", if console_more() { "..." } else { ">>>" }));
            input.clear();
            *back = 0;
            console_interrupt();
        }
    }
    // The cursor to the end of a line put in the field from outside it.
    let to_end = |ui: &egui::Ui, text: &str| {
        if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
            let end = egui::text::CCursor::new(text.chars().count());
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
            state.store(ui.ctx(), id);
        }
    };

    // Tab's answer: one completion goes in whole; several put in what they
    // have in common and are listed above, as a terminal lists them. Only if
    // the line is still the one asked about.
    if let Some((asked, head, matches)) = console_take_offer() {
        if asked == *input && !matches.is_empty() {
            let common = matches.iter().skip(1).fold(matches[0].clone(), |common, m| {
                common.chars().zip(m.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a).collect()
            });
            if common.len() > input.len() - head.len() {
                *input = format!("{head}{common}");
                to_end(ui, input);
            }
            if matches.len() > 1 {
                let names: Vec<&str> = matches.iter().map(|m| m.rsplit('.').next().unwrap_or(m)).collect();
                console_write(&format!("{}\n", names.join("  ")));
            }
        }
    }
    // Tab asks, rather than moving the focus on or typing a tab: taken
    // before the field sees it, and only while the field has the focus.
    if ui.memory(|m| m.has_focus(id)) && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
        console_ask_completion(input.clone());
    }

    // A row as tall as what is in it. `horizontal` starts a row at the
    // interaction height, 18 points, and the field used to be a line plus
    // its margins, 20 and a bit: centred, it stuck out past the panel's
    // bottom, egui kept the panel that much taller, and the panel grew every
    // frame to its maximum.
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let line = ui.fonts_mut(|f| f.row_height(&font)) + ui.spacing().extra_text_line_spacing;
    let height = line.max(ui.spacing().interact_size.y);
    let row = egui::vec2(ui.available_width(), height);
    ui.allocate_ui_with_layout(row, egui::Layout::left_to_right(egui::Align::Center), |ui| {
        // Python's own prompt colour, and what is typed coloured as it is
        // typed -- as Python 3.14's REPL does in a terminal.
        ui.label(egui::RichText::new(if console_more() { "..." } else { ">>>" }).monospace().color(palette.prompt));
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap: f32| {
            ui.ctx().fonts_mut(|f| f.layout_job(code::layout(buf.as_str(), code::Lang::Python, palette, font.clone())))
        };
        let edit = ui.add(
            // A terminal's line: the prompt, then what is typed, with no box
            // around it and no hint in it.
            egui::TextEdit::singleline(input)
                .id(id)
                .font(egui::TextStyle::Monospace)
                .frame(egui::Frame::NONE)
                .lock_focus(true)
                .layouter(&mut layouter)
                .desired_width(f32::INFINITY),
        );
        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let line = std::mem::take(input);
            if !line.trim().is_empty() && history.last() != Some(&line) {
                history.push(line.clone());
            }
            *back = 0;
            // Empty too: it is what ends a block.
            console_submit(line);
            edit.request_focus();
        } else if edit.has_focus() {
            let (up, down) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown)));
            if up && *back < history.len() {
                *back += 1;
            } else if down && *back > 0 {
                *back -= 1;
            } else {
                return;
            }
            *input = match *back {
                0 => String::new(),
                n => history[history.len() - n].clone(),
            };
            to_end(ui, input);
        }
    });
}

/// Where `script_write` sends the terminal's copy: the capture's saved
/// stdout while there is one, descriptor 1 again once it is gone.
#[cfg_attr(not(any(unix, windows)), allow(dead_code))]
fn script_terminal(terminal: Option<std::fs::File>) {
    SCRIPT_OUTPUT.lock().unwrap_or_else(|e| e.into_inner()).terminal = terminal.map(Terminal::new);
}

/// The reader thread: everything written to stdout and stderr goes on to the
/// terminal as it arrives, and each whole line is kept for the log, stamped
/// with the time it came off the pipe.
#[cfg(any(unix, windows))]
fn pump(
    mut reader: std::io::PipeReader,
    mut tee: Terminal,
    lines: &std::sync::Mutex<Vec<Entry>>,
    finished: std::sync::mpsc::Sender<()>,
) {
    use std::io::Read as _;

    // Bytes until a line is whole: a read can end inside a multibyte
    // character, which decoded on its own would print as garbage.
    let mut partial: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        tee.write(&buf[..n]);
        partial.extend_from_slice(&buf[..n]);
        let mut time = None;
        let mut whole = Vec::new();
        while let Some(i) = partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = partial.drain(..=i).collect();
            whole.push(Entry {
                time: time.get_or_insert_with(|| crate::app::clock::now().time()).clone(),
                text: String::from_utf8_lossy(&line).trim_end_matches(['\n', '\r']).to_string(),
            });
        }
        if !whole.is_empty() {
            lines.lock().unwrap_or_else(|e| e.into_inner()).extend(whole);
        }
    }
    if !partial.is_empty() {
        let last = Entry {
            time: crate::app::clock::now().time(),
            text: String::from_utf8_lossy(&partial).into_owned(),
        };
        lines.lock().unwrap_or_else(|e| e.into_inner()).push(last);
    }
    let _ = finished.send(());
}

#[cfg(test)]
mod terminal_tests {
    /// A read that ends inside a character keeps its start for the next one:
    /// decoded apart, an "é" cut in two printed as two replacement characters.
    #[test]
    fn a_character_cut_by_a_read_waits_for_its_end() {
        let e = "é".as_bytes();
        assert_eq!(super::whole_utf8(b"abc"), 3);
        assert_eq!(super::whole_utf8(&[b'a', e[0]]), 1, "the cut start waits");
        assert_eq!(super::whole_utf8(&[b'a', e[0], e[1]]), 3, "and goes once whole");
        assert_eq!(super::whole_utf8(&[b'a', 0xFF, b'b']), 3, "an invalid byte does not wait");
    }
}

// On Windows too since 26 September: the capture is `SetStdHandle` there.
#[cfg(all(test, any(unix, windows)))]
mod stdio_tests {
    use super::{log_tests::is_time, Log, StdioCapture};
    use std::time::{Duration, Instant};

    const CHILD: &str = "KALAST_STDIO_CAPTURE_CHILD";
    const NAME: &str =
        "app::gui::stdio_tests::more_than_a_pipe_holds_reaches_the_log_and_the_terminal";
    const LINES: usize = 4000;

    /// 4,000 lines, 280 KB, written with no frame to drain them -- a script
    /// named on the command line printing before the window exists. A pipe
    /// holds 64 KB, so with the frame as its only reader the write blocked
    /// for good. Every line has to reach the log, in order, and the terminal
    /// as well, the unterminated last one included.
    ///
    /// In a child process: a capture points this process's descriptors at a
    /// pipe, and every test running beside it would print into it.
    #[test]
    fn more_than_a_pipe_holds_reaches_the_log_and_the_terminal() {
        if std::env::var_os(CHILD).is_some() {
            return child();
        }
        let dir = std::env::temp_dir().join(format!("kalast-stdio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("terminal.txt");
        let file = std::fs::File::create(&path).unwrap();
        let mut run = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .stdout(file.try_clone().unwrap())
            .stderr(file)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = run.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = run.kill();
                panic!("the child hung: its output filled the pipe with nothing reading it");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let terminal = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(status.success(), "the child failed:\n{terminal}");
        // `contains`, not `starts_with`: the harness has written `test ... `
        // without a newline before the child's first line arrives.
        let teed = terminal.lines().filter(|l| l.contains("line ")).count();
        assert_eq!(teed, LINES, "lines on the terminal");
        assert!(terminal.contains("unterminated"), "the last, unterminated line");
        assert_eq!(terminal.matches("from the script").count(), 1, "the script's line, once");
        assert!(terminal.contains("the log has them all"), "the child's verdict");
    }

    fn child() {
        use std::io::Write as _;

        let mut capture = StdioCapture::new().expect("a capture");
        let mut out = std::io::stdout();
        for i in 0..LINES {
            writeln!(out, "line {i:04} {}", "x".repeat(60)).unwrap();
        }
        out.flush().unwrap();

        let mut log = Log::new(2 * LINES);
        let deadline = Instant::now() + Duration::from_secs(10);
        while log.lines().count() < LINES {
            assert!(Instant::now() < deadline, "{} lines reached the log", log.lines().count());
            capture.drain(&mut log);
            std::thread::sleep(Duration::from_millis(10));
        }
        for (i, line) in log.lines().enumerate() {
            assert!(line.starts_with(&format!("line {i:04} ")), "line {i} is {line:?}");
        }
        assert!(log.entries().all(|e| is_time(&e.time)), "the pipe's lines are stamped");
        // The way a script's `print` arrives in the UI app: past the pipe, to
        // the terminal and the script tab, in pieces as Python writes it.
        // Only now, with every line above off the pipe and on the terminal,
        // so the reader is idle: two threads writing there cut into each
        // other's lines -- the reader copies in chunks that end mid-line --
        // which failed this test on the Linux runner, a script line cut in
        // two, and here as a `line ` cut in two.
        super::script_write("from the ");
        super::script_write("script\n");
        let mut script = Log::new(8);
        super::drain_script_output(&mut script);
        assert!(script.entries().all(|e| is_time(&e.time)), "the script's lines are stamped");
        let script: Vec<&String> = script.lines().collect();
        assert_eq!(script, ["from the script"], "the script tab has the script's line, whole");

        // Last, a line with no end, which reaches the log only once the pipe
        // closes; and nothing of the script's came through the pipe.
        write!(out, "unterminated").unwrap();
        out.flush().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        capture.drain(&mut log);
        assert!(log.lines().all(|l| !l.contains("from the script")), "it leaked into the pipe");

        // Puts stdout back; what follows goes straight to the terminal.
        drop(capture);
        println!("the log has them all");
    }

    /// Runs the test named `name` again in a child process with `var` set,
    /// and fails if the child does: what these tests do to stdout is
    /// process-wide, and every test running beside them would print into it.
    #[cfg(windows)]
    fn in_a_child(name: &str, var: &str) {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(var, "1")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "the child failed:\n{text}");
    }

    /// Windows: stdout is the process's, and whatever else runs in it can
    /// point it elsewhere. kalast's own lines do not go through it
    /// (`engine_write`), so they reach the log whatever stdout is.
    #[cfg(windows)]
    #[test]
    fn a_moved_stdout_does_not_lose_the_engines_lines() {
        const MOVED: &str = "KALAST_STDIO_MOVED_CHILD";
        if std::env::var_os(MOVED).is_some() {
            return moved_child();
        }
        in_a_child("app::gui::stdio_tests::a_moved_stdout_does_not_lose_the_engines_lines", MOVED);
    }

    #[cfg(windows)]
    fn moved_child() {
        use std::io::Write as _;
        use std::os::windows::io::AsRawHandle as _;
        use windows_sys::Win32::System::Console::{SetStdHandle, STD_OUTPUT_HANDLE};

        let mut capture = StdioCapture::new().expect("a capture");
        let nul = std::fs::OpenOptions::new().write(true).open("NUL").unwrap();
        // SAFETY: `nul` outlives its use as stdout.
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, nul.as_raw_handle()) };
        writeln!(std::io::stdout(), "lost to NUL").unwrap();
        std::io::stdout().flush().unwrap();
        println!("the engine's line, stdout moved");

        let mut log = Log::new(16);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !log.lines().any(|l| l == "the engine's line, stdout moved") {
            assert!(Instant::now() < deadline, "the engine's line never arrived: {:?}", log.lines().collect::<Vec<_>>());
            std::thread::sleep(Duration::from_millis(10));
            capture.drain(&mut log);
        }
        assert!(!log.lines().any(|l| l.contains("lost to NUL")), "what went to NUL stays lost");
        drop(capture);
        drop(nul);
    }

    /// Windows: double-clicked, a program is handed the monitor to open on
    /// in its stdout slot, and `STARTF_HASSHELLDATA` to say so, which makes
    /// stdout read as NULL whatever the slot holds. What is printed to stdout
    /// has to reach the log all the same, and the process get both back as
    /// the shell left them. The child plays the shell's part on itself: the
    /// flag is all `GetStdHandle` looks at.
    #[cfg(windows)]
    #[test]
    fn a_double_clicked_stdout_reaches_the_log() {
        const SHELL: &str = "KALAST_STDIO_SHELL_CHILD";
        if std::env::var_os(SHELL).is_some() {
            return double_clicked_child();
        }
        in_a_child("app::gui::stdio_tests::a_double_clicked_stdout_reaches_the_log", SHELL);
    }

    #[cfg(windows)]
    fn double_clicked_child() {
        use std::io::Write as _;
        use windows_sys::Win32::System::Console::{GetStdHandle, SetStdHandle, STD_OUTPUT_HANDLE};

        // SAFETY: plain queries and sets of this process's standard handles,
        // here and below; nothing is written through the monitor, which
        // stdout never reads as.
        let harness = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        // The monitor a user's double-clicked kalast.exe was handed.
        let monitor = 0x10075usize as windows_sys::Win32::Foundation::HANDLE;
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, monitor) };
        super::give_back_shell_data();
        assert!(unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.is_null(), "stdout reads as NULL, as double-clicked");

        let mut capture = StdioCapture::new().expect("a capture");
        writeln!(std::io::stdout(), "printed double-clicked").unwrap();
        std::io::stdout().flush().unwrap();
        let mut log = Log::new(16);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !log.lines().any(|l| l == "printed double-clicked") {
            assert!(Instant::now() < deadline, "the line never reached the log: {:?}", log.lines().collect::<Vec<_>>());
            std::thread::sleep(Duration::from_millis(10));
            capture.drain(&mut log);
        }

        drop(capture);
        assert!(unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.is_null(), "stdout reads as NULL again");
        assert!(super::take_shell_data(), "because the flag is back");
        assert_eq!(unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }, monitor, "with the monitor in the slot");
        // The harness's stdout back, for its verdict.
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, harness) };
    }
}

impl Drop for StdioCapture {
    fn drop(&mut self) {
        self.restore();
    }
}


#[cfg(test)]
mod scripts_tests {
    use super::*;

    /// A folder of its own for each test, gone after it.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kalast-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The editor's files: an edit survives switching away and back, a file
    /// closed gives its place to the next, a renamed one is followed, and
    /// what is not text is refused rather than opened.
    #[test]
    fn switching_files_keeps_their_edits() {
        let dir = scratch("buffers");
        let (a, b) = (dir.join("a.py"), dir.join("b.py"));
        std::fs::write(&a, "a = 1\n").unwrap();
        std::fs::write(&b, "b = 2\n").unwrap();
        std::fs::write(dir.join("mesh.bin"), [1u8, 0, 2]).unwrap();
        let (mut script, mut path, mut dirty) = (String::new(), String::new(), false);
        let (mut opened, mut stash) = (Vec::new(), std::collections::HashMap::new());
        let mut files =
            Buffers { script: &mut script, path: &mut path, dirty: &mut dirty, opened: &mut opened, stash: &mut stash };
        let key = |p: &std::path::Path| p.display().to_string();

        files.open(&a).unwrap();
        assert_eq!((files.script.as_str(), files.path.as_str()), ("a = 1\n", key(&a).as_str()));
        *files.script = "a = 10\n".to_string();
        *files.dirty = true;

        files.open(&b).unwrap();
        assert_eq!((files.script.as_str(), *files.dirty), ("b = 2\n", false));
        assert_eq!(*files.opened, [key(&a), key(&b)]);

        files.open(&a).unwrap();
        assert_eq!((files.script.as_str(), *files.dirty), ("a = 10\n", true), "the edit kept");

        files.close(&key(&a));
        assert_eq!((files.path.as_str(), files.script.as_str()), (key(&b).as_str(), "b = 2\n"), "the next one shown");
        assert_eq!(*files.opened, [key(&b)]);

        let c = dir.join("c.py");
        files.renamed(&key(&b), &key(&c));
        assert_eq!((files.path.as_str(), files.opened.as_slice()), (key(&c).as_str(), [key(&c)].as_slice()));
        // Its folder renamed: followed too, by what is under it.
        let moved_dir = dir.with_file_name(format!("{}-moved", dir.file_name().unwrap().to_string_lossy()));
        files.renamed(&key(&dir), &key(&moved_dir));
        assert_eq!(files.path.as_str(), key(&moved_dir.join("c.py")));
        files.renamed(&key(&moved_dir), &key(&dir));

        assert!(files.open(&dir.join("mesh.bin")).unwrap_err().contains("not text"));
        assert!(files.open(&dir.join("gone.py")).is_err());
        assert_eq!(files.path.as_str(), key(&c), "a refused file changes nothing");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Names typed into the tree: a folder, an empty file and a new name
    /// made; a name taken or not a name refused, the field kept for it.
    #[test]
    fn names_typed_in_the_tree_make_what_they_say() {
        let dir = scratch("naming");
        let mut tree = FileTree::default();
        let mut action = None;
        let name = |tree: &mut FileTree, what: Named, text: &str, action: &mut Option<TreeAction>| {
            tree.start(what);
            tree.naming.as_mut().unwrap().text = text.to_string();
            tree.commit(true, action);
        };

        name(&mut tree, Named::Folder(dir.clone()), "orbit", &mut action);
        assert!(dir.join("orbit").is_dir() && tree.naming.is_none() && action.is_none());

        name(&mut tree, Named::File(dir.join("orbit")), "main.py", &mut action);
        let made = dir.join("orbit").join("main.py");
        assert!(made.is_file());
        assert_eq!(action.take(), Some(TreeAction::Created(made.clone())));

        name(&mut tree, Named::Rename(made.clone()), "run.py", &mut action);
        let renamed = dir.join("orbit").join("run.py");
        assert!(renamed.is_file() && !made.exists());
        assert_eq!(action.take(), Some(TreeAction::Renamed(made, renamed.clone())));

        // A folder renamed, open, and what is open in it kept open.
        tree.open.insert(dir.join("orbit"));
        name(&mut tree, Named::Rename(dir.join("orbit")), "transfer", &mut action);
        assert!(dir.join("transfer").join("run.py").is_file() && tree.open.contains(&dir.join("transfer")));
        assert_eq!(action.take(), Some(TreeAction::Renamed(dir.join("orbit"), dir.join("transfer"))));
        name(&mut tree, Named::Rename(dir.join("transfer")), "orbit", &mut action);
        action = None;

        std::fs::write(dir.join("orbit").join("taken.py"), "").unwrap();
        name(&mut tree, Named::Rename(renamed.clone()), "taken.py", &mut action);
        assert!(tree.naming.as_ref().is_some_and(|n| n.error.is_some()), "a name taken: said, the field kept");
        assert!(renamed.is_file());
        tree.naming = None;
        name(&mut tree, Named::File(dir.clone()), "a/b.py", &mut action);
        assert!(tree.naming.as_ref().is_some_and(|n| n.error.is_some()), "not a name");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Text is read; what is not -- a NUL in it, or not UTF-8 -- or is too
    /// big to edit, is said, not opened.
    #[test]
    fn only_text_is_read_for_the_editor() {
        let dir = scratch("text");
        std::fs::write(dir.join("a.md"), "# Title\n").unwrap();
        std::fs::write(dir.join("latin1.txt"), [0x63, 0x61, 0x66, 0xe9]).unwrap();
        std::fs::write(dir.join("big.obj"), vec![b'v'; 5 << 20]).unwrap();
        assert_eq!(read_text(&dir.join("a.md")).unwrap(), "# Title\n");
        assert!(read_text(&dir.join("latin1.txt")).unwrap_err().contains("UTF-8"));
        assert!(read_text(&dir.join("big.obj")).unwrap_err().contains("too big"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
