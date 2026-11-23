//! Files, through one door: `std::fs` on the desktop, and in a browser, which
//! gives a page no file system, the page's own.
//!
//! The desktop's functions are `std::fs`'s, re-exported, and the few helpers
//! below are what their call sites did with them -- nothing changes there.
//! In a browser the files are a tree in memory: the examples and resources
//! the page was built with, put in by the page before kalast starts (`add`),
//! and what is made or saved since, which the page is told of (`on_change`)
//! so that it can keep it for the next visit.
//!
//! Only what kalast itself reads and writes goes through here: the editor's
//! files, the meshes a script loads, the settings the UI app remembers.

#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
pub use std::fs::{create_dir, create_dir_all, read, read_to_string, rename, write};

/// `path`'s size in bytes.
#[cfg(not(target_arch = "wasm32"))]
pub fn len(path: &Path) -> io::Result<u64> {
    std::fs::metadata(path).map(|m| m.len())
}

/// The entries of `dir`, each one's name and whether it is a folder.
#[cfg(not(target_arch = "wasm32"))]
pub fn list(dir: &Path) -> io::Result<Vec<(String, bool)>> {
    Ok(std::fs::read_dir(dir)?
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.file_type().is_ok_and(|t| t.is_dir())))
        .collect())
}

/// Make `path`, an empty file, refused when something is there already.
#[cfg(not(target_arch = "wasm32"))]
pub fn create_new(path: &Path) -> io::Result<()> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).map(|_| ())
}

/// `path` to read through, buffered by `capacity` bytes.
#[cfg(not(target_arch = "wasm32"))]
pub fn reader(path: &Path, capacity: usize) -> io::Result<impl io::BufRead> {
    Ok(io::BufReader::with_capacity(capacity, std::fs::File::open(path)?))
}

#[cfg(target_arch = "wasm32")]
pub use page::*;

// Built for the tests too, which check it on the desktop.
#[cfg(any(test, target_arch = "wasm32"))]
mod page {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Component, Path};

    /// What a page's file system holds at a path.
    enum Node {
        Folder,
        File(Vec<u8>),
    }

    /// What was made, saved or taken away, for the page to keep.
    pub enum Change<'a> {
        File(&'a [u8]),
        Folder,
        Removed,
    }

    type Listener = Box<dyn Fn(&str, Change<'_>)>;

    thread_local! {
        /// Every file and folder, by its path from the page's root, `/` between
        /// names: a folder's entries are the keys that start with its own and
        /// a `/`. The root is the empty key, and always there.
        static TREE: RefCell<BTreeMap<String, Node>> = RefCell::new(BTreeMap::from([(String::new(), Node::Folder)]));
        static LISTENER: RefCell<Option<Listener>> = const { RefCell::new(None) };
    }

    /// `path` as `TREE` keys it: from the root, `.` and `..` resolved, an
    /// absolute path taken from the root too -- the page's root is all the
    /// root there is.
    fn key(path: &Path) -> String {
        let mut names: Vec<String> = Vec::new();
        for c in path.components() {
            match c {
                Component::Normal(name) => names.push(name.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    names.pop();
                }
                Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            }
        }
        names.join("/")
    }

    fn parent(key: &str) -> &str {
        key.rfind('/').map_or("", |at| &key[..at])
    }

    fn missing(key: &str) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, format!("no file or folder {key:?}"))
    }

    fn told(key: &str, change: Change<'_>) {
        LISTENER.with(|l| {
            if let Some(listener) = l.borrow().as_ref() {
                listener(key, change);
            }
        });
    }

    /// Put a file in, its folders made as needed: what the page was built
    /// with, before kalast starts, or kept from an earlier visit. Not a
    /// change to tell the page of.
    pub fn add(path: &str, bytes: Vec<u8>) {
        let key = key(Path::new(path));
        TREE.with(|t| {
            let mut tree = t.borrow_mut();
            let mut at = parent(&key);
            while !at.is_empty() {
                tree.entry(at.to_string()).or_insert(Node::Folder);
                at = parent(at);
            }
            tree.insert(key, Node::File(bytes));
        });
    }

    /// Put a folder in, as `add` does a file.
    pub fn add_folder(path: &str) {
        let key = key(Path::new(path));
        TREE.with(|t| {
            let mut tree = t.borrow_mut();
            let mut at = key.as_str();
            while !at.is_empty() {
                tree.entry(at.to_string()).or_insert(Node::Folder);
                at = parent(at);
            }
        });
    }

    /// Be told of every file and folder made, saved or taken away from here
    /// on, by its path from the root.
    pub fn on_change(listener: impl Fn(&str, Change<'_>) + 'static) {
        LISTENER.with(|l| *l.borrow_mut() = Some(Box::new(listener)));
    }

    pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let key = key(path.as_ref());
        TREE.with(|t| match t.borrow().get(&key) {
            Some(Node::File(bytes)) => Ok(bytes.clone()),
            Some(Node::Folder) => Err(io::Error::new(io::ErrorKind::IsADirectory, format!("{key:?} is a folder"))),
            None => Err(missing(&key)),
        })
    }

    pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
        String::from_utf8(read(path)?).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-8"))
    }

    /// As `std::fs::write`: the folder must be there, and a file there is
    /// replaced.
    pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
        let key = key(path.as_ref());
        let bytes = contents.as_ref().to_vec();
        TREE.with(|t| {
            let mut tree = t.borrow_mut();
            if key.is_empty() || matches!(tree.get(&key), Some(Node::Folder)) {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, format!("{key:?} is a folder")));
            }
            if !matches!(tree.get(parent(&key)), Some(Node::Folder)) {
                return Err(missing(parent(&key)));
            }
            tree.insert(key.clone(), Node::File(bytes));
            Ok(())
        })?;
        told(&key, Change::File(contents.as_ref()));
        Ok(())
    }

    pub fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
        let key = key(path.as_ref());
        TREE.with(|t| {
            let mut tree = t.borrow_mut();
            if tree.contains_key(&key) {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{key:?} is there already")));
            }
            if !matches!(tree.get(parent(&key)), Some(Node::Folder)) {
                return Err(missing(parent(&key)));
            }
            tree.insert(key.clone(), Node::Folder);
            Ok(())
        })?;
        told(&key, Change::Folder);
        Ok(())
    }

    pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
        let key = key(path.as_ref());
        let mut made = Vec::new();
        TREE.with(|t| {
            let mut tree = t.borrow_mut();
            let mut at = key.as_str();
            let mut chain = Vec::new();
            while !at.is_empty() {
                chain.push(at.to_string());
                at = parent(at);
            }
            for k in chain.into_iter().rev() {
                match tree.get(&k) {
                    Some(Node::Folder) => {}
                    Some(Node::File(_)) => {
                        return Err(io::Error::new(io::ErrorKind::NotADirectory, format!("{k:?} is a file")));
                    }
                    None => {
                        tree.insert(k.clone(), Node::Folder);
                        made.push(k);
                    }
                }
            }
            Ok(())
        })?;
        for k in made {
            told(&k, Change::Folder);
        }
        Ok(())
    }

    /// Move a file or a folder with all it holds; a file at `to` is
    /// replaced, as on Unix.
    pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
        let (from, to) = (key(from.as_ref()), key(to.as_ref()));
        let moved: Vec<(String, String)> = TREE.with(|t| {
            let mut tree = t.borrow_mut();
            if from.is_empty() || !tree.contains_key(&from) {
                return Err(missing(&from));
            }
            if !matches!(tree.get(parent(&to)), Some(Node::Folder)) {
                return Err(missing(parent(&to)));
            }
            if to == from || to.starts_with(&format!("{from}/")) {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{to:?} is inside {from:?}")));
            }
            if matches!(tree.get(&to), Some(Node::Folder)) {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{to:?} is a folder")));
            }
            let under = format!("{from}/");
            let keys: Vec<String> =
                tree.keys().filter(|k| **k == from || k.starts_with(&under)).cloned().collect();
            let mut moved = Vec::new();
            for k in keys {
                let node = tree.remove(&k).unwrap();
                let new = format!("{to}{}", &k[from.len()..]);
                tree.insert(new.clone(), node);
                moved.push((k, new));
            }
            Ok(moved)
        })?;
        for (old, new) in moved {
            told(&old, Change::Removed);
            TREE.with(|t| match t.borrow().get(&new) {
                Some(Node::File(bytes)) => told(&new, Change::File(bytes)),
                Some(Node::Folder) => told(&new, Change::Folder),
                None => {}
            });
        }
        Ok(())
    }

    /// Take away a file, or a folder with all it holds: what a page has
    /// for a move to the Trash.
    pub fn remove_all(path: impl AsRef<Path>) -> io::Result<()> {
        let key = key(path.as_ref());
        if key.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "the root stays"));
        }
        let under = format!("{key}/");
        let gone: Vec<String> = TREE.with(|t| {
            let mut tree = t.borrow_mut();
            let keys: Vec<String> = tree.keys().filter(|k| **k == key || k.starts_with(&under)).cloned().collect();
            for k in &keys {
                tree.remove(k);
            }
            keys
        });
        if gone.is_empty() {
            return Err(missing(&key));
        }
        for k in gone {
            told(&k, Change::Removed);
        }
        Ok(())
    }

    pub fn len(path: &Path) -> io::Result<u64> {
        let key = key(path);
        TREE.with(|t| match t.borrow().get(&key) {
            Some(Node::File(bytes)) => Ok(bytes.len() as u64),
            Some(Node::Folder) => Ok(0),
            None => Err(missing(&key)),
        })
    }

    pub fn list(dir: &Path) -> io::Result<Vec<(String, bool)>> {
        let key = key(dir);
        let prefix = if key.is_empty() { String::new() } else { format!("{key}/") };
        TREE.with(|t| {
            let tree = t.borrow();
            match tree.get(&key) {
                Some(Node::Folder) => {}
                Some(Node::File(_)) => {
                    return Err(io::Error::new(io::ErrorKind::NotADirectory, format!("{key:?} is a file")));
                }
                None => return Err(missing(&key)),
            }
            Ok(tree
                .range(prefix.clone()..)
                .take_while(|(k, _)| k.starts_with(&prefix))
                .filter(|(k, _)| !k.is_empty() && !k[prefix.len()..].contains('/'))
                .map(|(k, node)| (k[prefix.len()..].to_string(), matches!(node, Node::Folder)))
                .collect())
        })
    }

    pub fn create_new(path: &Path) -> io::Result<()> {
        let key = key(path);
        if TREE.with(|t| t.borrow().contains_key(&key)) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{key:?} is there already")));
        }
        write(path, [])
    }

    pub fn reader(path: &Path, _capacity: usize) -> io::Result<impl io::BufRead> {
        Ok(io::Cursor::new(read(path)?))
    }

    /// Whether `path` is there, file or folder.
    pub fn exists(path: &Path) -> bool {
        let key = key(path);
        TREE.with(|t| t.borrow().contains_key(&key))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn the_desktops_helpers_say_what_std_does() {
        let dir = std::env::temp_dir().join(format!("kalast-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        create_dir_all(dir.join("a")).unwrap();
        write(dir.join("b.txt"), "hello").unwrap();
        create_new(&dir.join("c.txt")).unwrap();
        assert!(create_new(&dir.join("c.txt")).is_err(), "a file there already is refused");
        assert_eq!(len(&dir.join("b.txt")).unwrap(), 5);
        let mut entries = list(&dir).unwrap();
        entries.sort();
        assert_eq!(entries, [("a".to_string(), true), ("b.txt".to_string(), false), ("c.txt".to_string(), false)]);
        let mut text = String::new();
        io::Read::read_to_string(&mut reader(&dir.join("b.txt"), 16).unwrap(), &mut text).unwrap();
        assert_eq!(text, "hello");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // A page's tree, on the desktop: each test runs on a thread of its own,
    // and so has a tree of its own.

    #[test]
    fn a_pages_tree_lists_and_reads_what_it_was_given() {
        page::add("examples/cube/light.py", b"print(1)\n".to_vec());
        page::add("examples/README.md", b"# Examples\n".to_vec());
        page::add_folder("scripts");
        assert_eq!(page::list(Path::new("")).unwrap(), [("examples".to_string(), true), ("scripts".to_string(), true)]);
        assert_eq!(page::list(Path::new(".")).unwrap(), page::list(Path::new("")).unwrap());
        assert_eq!(
            page::list(Path::new("examples")).unwrap(),
            [("README.md".to_string(), false), ("cube".to_string(), true)]
        );
        assert_eq!(page::read_to_string("./examples/../examples/cube/light.py").unwrap(), "print(1)\n");
        assert_eq!(page::read_to_string("/examples/README.md").unwrap(), "# Examples\n", "absolute is from the root");
        assert_eq!(page::len(Path::new("examples/README.md")).unwrap(), 11);
        assert_eq!(page::read("examples").unwrap_err().kind(), io::ErrorKind::IsADirectory);
        assert_eq!(page::read("nothing.py").unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(page::list(Path::new("examples/README.md")).is_err());
    }

    #[test]
    fn a_pages_tree_writes_moves_and_takes_away_as_std_would() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log = seen.clone();
        page::on_change(move |key, change| {
            let what = match change {
                page::Change::File(b) => format!("file {}", b.len()),
                page::Change::Folder => "folder".to_string(),
                page::Change::Removed => "removed".to_string(),
            };
            log.borrow_mut().push(format!("{key}: {what}"));
        });
        assert_eq!(page::write("scripts/a.py", "x").unwrap_err().kind(), io::ErrorKind::NotFound, "no folder yet");
        page::create_dir_all("scripts/orbit").unwrap();
        page::write("scripts/orbit/a.py", "x = 1\n").unwrap();
        assert!(page::create_dir("scripts/orbit").is_err());
        page::create_new(Path::new("scripts/b.py")).unwrap();
        assert_eq!(page::create_new(Path::new("scripts/b.py")).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        page::rename("scripts/orbit", "scripts/moon").unwrap();
        assert_eq!(page::read_to_string("scripts/moon/a.py").unwrap(), "x = 1\n");
        assert!(!page::exists(Path::new("scripts/orbit")));
        assert!(page::rename("scripts", "scripts/inside").is_err(), "not into itself");
        page::remove_all("scripts/moon").unwrap();
        assert_eq!(page::list(Path::new("scripts")).unwrap(), [("b.py".to_string(), false)]);
        assert_eq!(
            *seen.borrow(),
            [
                "scripts: folder",
                "scripts/orbit: folder",
                "scripts/orbit/a.py: file 6",
                "scripts/b.py: file 0",
                "scripts/orbit: removed",
                "scripts/moon: folder",
                "scripts/orbit/a.py: removed",
                "scripts/moon/a.py: file 6",
                "scripts/moon: removed",
                "scripts/moon/a.py: removed",
            ]
        );
    }
}
