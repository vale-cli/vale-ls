use core::fmt;
use std::{collections::HashSet, fs, path::PathBuf};

use crate::error::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Style,
    Vocab,
    Rule,
}

#[derive(Debug, Clone)]
pub struct PathEntry {
    pub name: String,
    pub size: usize,
    pub path: PathBuf,
    pub kind: EntryType,
}

#[derive(Debug)]
pub struct StylesPath {
    roots: Vec<PathBuf>,
}

impl fmt::Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EntryType::Style => write!(f, "Style"),
            EntryType::Vocab => write!(f, "Vocab"),
            EntryType::Rule => write!(f, "Rule"),
        }
    }
}

/// `StylesPath` provides an interface for managing one or more directories of
/// styles.
impl StylesPath {
    pub fn new(roots: Vec<PathBuf>) -> StylesPath {
        StylesPath { roots }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.roots = vec![path];
    }

    /// The project-specific styles directory -- i.e., the last path Vale
    /// searches.
    pub fn path(&self) -> PathBuf {
        self.roots.last().cloned().unwrap_or_default()
    }

    pub fn add_to_accept(&self, name: &str, term: &str) -> Result<(), Error> {
        self.add_to_vocab(name, term, true)
    }

    pub fn add_to_reject(&self, name: &str, term: &str) -> Result<(), Error> {
        self.add_to_vocab(name, term, false)
    }

    pub fn count(&self, kind: EntryType) -> Result<usize, Error> {
        let idx = self.index()?;
        Ok(idx.iter().filter(|e| e.kind == kind).count())
    }

    pub fn get_vocab(&self) -> Result<Vec<PathEntry>, Error> {
        self.get(EntryType::Vocab)
    }

    pub fn get_styles(&self) -> Result<Vec<PathEntry>, Error> {
        let mut styles = vec![PathEntry {
            name: "Vale".to_string(),
            size: 4,
            path: "".into(),
            kind: EntryType::Style,
        }];
        styles.append(&mut self.get(EntryType::Style)?);

        Ok(styles)
    }

    pub fn has(&self, path: &str) -> Result<bool, Error> {
        let idx = self.index()?;
        Ok(idx.iter().any(|e| e.path.to_string_lossy() == path))
    }

    fn get(&self, kind: EntryType) -> Result<Vec<PathEntry>, Error> {
        let idx = self.index()?;
        let mut seen = HashSet::new();

        // A style or vocab may live in more than one search path; `index`
        // yields the most specific one first, so we keep that.
        Ok(idx
            .into_iter()
            .filter(|e| e.kind == kind)
            .filter(|e| seen.insert(e.name.clone()))
            .collect())
    }

    fn add_to_vocab(&self, name: &str, term: &str, accept: bool) -> Result<(), Error> {
        let root = self.path();

        // `Vocab` is the pre-v3 location of `config/vocabularies`.
        let mut dir = root.join("config").join("vocabularies").join(name);
        if !dir.is_dir() && root.join("Vocab").join(name).is_dir() {
            dir = root.join("Vocab").join(name);
        }

        let path = dir.join(if accept { "accept.txt" } else { "reject.txt" });

        // Vale creates these on `sync`, but a project may have a vocabulary
        // it hasn't written a term to yet.
        fs::create_dir_all(&dir)?;
        let content = fs::read_to_string(&path).unwrap_or_default();

        let mut lines = content.lines().collect::<Vec<_>>();
        if lines.contains(&term) {
            return Ok(());
        }

        lines.push(term);
        lines.sort_by_key(|line| line.to_lowercase());

        fs::write(path, lines.join("\n") + "\n")?;

        Ok(())
    }

    fn index(&self) -> Result<Vec<PathEntry>, Error> {
        let mut entries = Vec::new();

        // Most specific search path first, so that `get` reports the entry
        // Vale would actually use when a name appears in more than one.
        for root in self.roots.iter().rev() {
            // Vale reports all of its search paths, some of which may not
            // exist yet (e.g., a global styles directory).
            if !root.is_dir() {
                continue;
            }

            for path in fs::read_dir(root)? {
                let subdir = path?;
                let path = subdir.path();

                let dir_name = self.entry_name(path.clone());
                if dir_name == ".vale-config" {
                    continue;
                } else if dir_name == "config" && path.is_dir() {
                    // Vale >= 3.0 keeps vocabularies (and other assets) in
                    // `config/`, which isn't a style.
                    let vocab = path.join("vocabularies");
                    if vocab.is_dir() {
                        entries.append(&mut self.index_dir(vocab, EntryType::Vocab)?);
                    }
                } else if dir_name == "Vocab" && path.is_dir() {
                    entries.append(&mut self.index_dir(path.clone(), EntryType::Vocab)?);
                } else if path.is_dir() {
                    entries.push(PathEntry {
                        name: dir_name,
                        size: fs::read_dir(path.clone()).unwrap().count(),
                        path: path.clone(),
                        kind: EntryType::Style,
                    });
                    entries.append(&mut self.index_dir(path.clone(), EntryType::Rule)?);
                }
            }
        }

        Ok(entries)
    }

    fn entry_name(&self, path: PathBuf) -> String {
        path.file_name()
            .unwrap_or("".as_ref())
            .to_string_lossy()
            .to_string()
    }

    fn index_dir(&self, path: PathBuf, kind: EntryType) -> Result<Vec<PathEntry>, Error> {
        let mut entries = vec![];

        fs::read_dir(path)?
            .into_iter()
            .filter(|r| r.is_ok())
            .map(|r| r.unwrap().path())
            .for_each({
                |path| {
                    let ext = path.extension().unwrap_or("".as_ref());
                    if ext == "yml" || (path.is_dir() && kind == EntryType::Vocab) {
                        entries.push(PathEntry {
                            name: self.entry_name(path.clone()),
                            size: 0,
                            path: path.clone(),
                            kind: kind.clone(),
                        });
                    }
                }
            });

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: &str = ".github/styles";

    #[test]
    fn index() {
        let p = StylesPath::new(vec![PathBuf::from(STYLES)]);

        assert_eq!(p.count(EntryType::Style).unwrap(), 2);
        assert_eq!(p.count(EntryType::Rule).unwrap(), 8);
        assert_eq!(p.count(EntryType::Vocab).unwrap(), 1);

        let style = p
            .get_styles()
            .unwrap()
            .into_iter()
            .find(|s| s.name == "Test")
            .unwrap();

        assert_eq!(style.name, "Test");
        assert_eq!(style.size, 1);
    }

    #[test]
    fn index_skips_missing_paths() {
        let p = StylesPath::new(vec![PathBuf::from("does-not-exist"), PathBuf::from(STYLES)]);

        assert_eq!(p.count(EntryType::Style).unwrap(), 2);
        assert_eq!(p.count(EntryType::Rule).unwrap(), 8);
        assert_eq!(p.count(EntryType::Vocab).unwrap(), 1);
    }

    #[test]
    fn modern_vocab_layout() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join("config").join("vocabularies").join("Proj")).unwrap();
        fs::create_dir_all(root.join("MyStyle")).unwrap();
        fs::write(root.join("MyStyle").join("Rule.yml"), "extends: existence").unwrap();

        let p = StylesPath::new(vec![root.to_path_buf()]);

        // `config` holds vocabularies, dictionaries, etc. -- not styles.
        assert_eq!(p.count(EntryType::Style).unwrap(), 1);
        assert_eq!(p.count(EntryType::Rule).unwrap(), 1);

        let vocab = p.get_vocab().unwrap();
        assert_eq!(vocab.len(), 1);
        assert_eq!(vocab[0].name, "Proj");
    }

    #[test]
    fn add_to_vocab_writes_sorted_terms() {
        let dir = tempfile::tempdir().unwrap();
        let vocab = dir.path().join("config").join("vocabularies").join("Proj");

        fs::create_dir_all(&vocab).unwrap();
        fs::write(vocab.join("accept.txt"), "beta\n").unwrap();

        let p = StylesPath::new(vec![dir.path().to_path_buf()]);
        p.add_to_accept("Proj", "alpha").unwrap();
        p.add_to_accept("Proj", "Gamma").unwrap();

        // Sorted case-insensitively, and Vale wants a trailing newline.
        let content = fs::read_to_string(vocab.join("accept.txt")).unwrap();
        assert_eq!(content, "alpha\nbeta\nGamma\n");

        // Adding a term twice is a no-op, not a duplicate line.
        p.add_to_accept("Proj", "alpha").unwrap();
        assert_eq!(
            fs::read_to_string(vocab.join("accept.txt")).unwrap(),
            "alpha\nbeta\nGamma\n"
        );

        // A vocabulary that has no file yet still works.
        p.add_to_reject("Proj", "nope").unwrap();
        assert_eq!(
            fs::read_to_string(vocab.join("reject.txt")).unwrap(),
            "nope\n"
        );
    }

    #[test]
    fn add_to_vocab_honors_the_legacy_layout() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("Vocab").join("Proj");
        fs::create_dir_all(&legacy).unwrap();

        let p = StylesPath::new(vec![dir.path().to_path_buf()]);
        p.add_to_accept("Proj", "alpha").unwrap();

        assert_eq!(
            fs::read_to_string(legacy.join("accept.txt")).unwrap(),
            "alpha\n"
        );
        assert!(!dir.path().join("config").exists());
    }

    #[test]
    fn duplicates_are_reported_once() {
        let p = StylesPath::new(vec![PathBuf::from(STYLES), PathBuf::from(STYLES)]);

        // "Vale" is built in, so it's always included.
        assert_eq!(p.get_styles().unwrap().len(), 3);
        assert_eq!(p.get_vocab().unwrap().len(), 1);
    }
}
