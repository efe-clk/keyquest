//! Layout and lesson repositories: embedded defaults merged with the user's
//! own TOML files. A user file with the same `id` replaces the default; a
//! broken user file is skipped with a warning (KR-4).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use homerow_core::{Layout, LayoutDef, LayoutOptions, Lesson};

use crate::embedded;
use crate::{DataError, Result};

/// User data files larger than this are rejected.
pub const MAX_FILE_SIZE: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub source: String,
    pub message: String,
}

/// `.toml` files under `dir`, up to `depth` directory levels deep, sorted.
fn toml_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            if depth > 0 {
                toml_files(&p, depth - 1, out);
            }
        } else if p.extension().is_some_and(|e| e == "toml") {
            out.push(p);
        }
    }
}

fn read_user_file(path: &Path) -> std::result::Result<String, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(format!("dosya çok büyük ({} bayt)", meta.len()));
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Parses embedded files, then user files from `user_dir`, keyed by id.
fn load<T>(
    embedded: &[(&str, &str)],
    user_dir: Option<&Path>,
    depth: usize,
    parse: impl Fn(&str) -> std::result::Result<T, String>,
    id: impl Fn(&T) -> &str,
) -> (BTreeMap<String, T>, Vec<Warning>) {
    let mut items = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut add = |source: String, text: std::result::Result<String, String>| match text
        .and_then(|t| parse(&t))
    {
        Ok(item) => {
            items.insert(id(&item).to_owned(), item);
        }
        Err(message) => warnings.push(Warning { source, message }),
    };
    for (name, text) in embedded {
        add(format!("(gömülü) {name}"), Ok(text.to_string()));
    }
    if let Some(dir) = user_dir {
        let mut files = Vec::new();
        toml_files(dir, depth, &mut files);
        for f in files {
            add(f.display().to_string(), read_user_file(&f));
        }
    }
    (items, warnings)
}

#[derive(Debug, Clone)]
pub struct LayoutRepository {
    defs: BTreeMap<String, LayoutDef>,
    warnings: Vec<Warning>,
}

impl LayoutRepository {
    pub fn embedded() -> LayoutRepository {
        LayoutRepository::load(None)
    }

    /// Embedded layouts plus `*.toml` files in `user_dir` (if it exists).
    pub fn load(user_dir: Option<&Path>) -> LayoutRepository {
        let (defs, warnings) = load(
            embedded::LAYOUTS,
            user_dir,
            0,
            |t| LayoutDef::from_toml(t).map_err(|e| e.to_string()),
            |d: &LayoutDef| &d.id,
        );
        LayoutRepository { defs, warnings }
    }

    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    pub fn defs(&self) -> impl Iterator<Item = &LayoutDef> {
        self.defs.values()
    }

    pub fn layout(&self, id: &str, opts: &LayoutOptions) -> Result<Layout> {
        let def = self
            .defs
            .get(id)
            .ok_or_else(|| DataError::UnknownLayout(id.to_owned()))?;
        Ok(Layout::from_def(def.clone(), opts)?)
    }
}

#[derive(Debug, Clone)]
pub struct LessonRepository {
    lessons: BTreeMap<String, Lesson>,
    warnings: Vec<Warning>,
}

impl LessonRepository {
    pub fn embedded() -> LessonRepository {
        LessonRepository::load(None)
    }

    /// Embedded lessons plus `*.toml` files in `user_dir` and its direct
    /// subdirectories (e.g. `lessons/tr-q/10-sayilar.toml`).
    pub fn load(user_dir: Option<&Path>) -> LessonRepository {
        let (lessons, warnings) = load(
            embedded::LESSONS,
            user_dir,
            1,
            |t| Lesson::from_toml(t).map_err(|e| e.to_string()),
            |l: &Lesson| &l.id,
        );
        LessonRepository { lessons, warnings }
    }

    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    pub fn all(&self) -> impl Iterator<Item = &Lesson> {
        self.lessons.values()
    }

    pub fn get(&self, id: &str) -> Option<&Lesson> {
        self.lessons.get(id)
    }

    /// Lessons for a layout in order (sorted by id, so `01-...` comes first).
    pub fn for_layout(&self, layout: &str) -> Vec<&Lesson> {
        self.lessons
            .values()
            .filter(|l| l.layout == layout)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_layouts_load() {
        let repo = LayoutRepository::embedded();
        assert!(repo.warnings().is_empty(), "{:?}", repo.warnings());
        let ids: Vec<_> = repo.defs().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["tr-f", "tr-q", "us"]);
        for id in ids {
            repo.layout(id, &LayoutOptions::default()).unwrap();
        }
        assert!(matches!(
            repo.layout("xx", &LayoutOptions::default()),
            Err(DataError::UnknownLayout(_))
        ));
    }

    #[test]
    fn embedded_lessons_load() {
        let repo = LessonRepository::embedded();
        assert!(repo.warnings().is_empty(), "{:?}", repo.warnings());
        assert!(repo.for_layout("tr-q").len() >= 6);
    }

    #[test]
    fn every_data_file_is_embedded() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for (dir, depth, embedded) in [
            ("data/layouts", 0, embedded::LAYOUTS),
            ("data/lessons", 1, embedded::LESSONS),
        ] {
            let mut files = Vec::new();
            toml_files(&root.join(dir), depth, &mut files);
            let mut on_disk: Vec<String> = files
                .iter()
                .map(|f| f.strip_prefix(root).unwrap().display().to_string())
                .collect();
            let mut listed: Vec<String> = embedded.iter().map(|(n, _)| n.to_string()).collect();
            on_disk.sort();
            listed.sort();
            assert_eq!(on_disk, listed, "src/embedded.rs güncel değil");
        }
    }

    #[test]
    fn user_files_override_and_bad_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let tr = dir.path().join("tr-q");
        fs::create_dir(&tr).unwrap();
        fs::write(
            tr.join("01.toml"),
            embedded::LESSONS[0]
                .1
                .replace("Ana sıra: a s d f j k l ş", "Benim dersim"),
        )
        .unwrap();
        fs::write(tr.join("bozuk.toml"), "id = ").unwrap();
        fs::write(tr.join("notlar.txt"), "yok sayılır").unwrap();
        fs::write(
            dir.path().join("buyuk.toml"),
            "#".repeat(MAX_FILE_SIZE as usize + 1),
        )
        .unwrap();

        let repo = LessonRepository::load(Some(dir.path()));
        assert_eq!(repo.get("tr-q/01-ana-sira").unwrap().title, "Benim dersim");
        assert_eq!(repo.warnings().len(), 2, "{:?}", repo.warnings());
        assert!(repo.warnings().iter().any(|w| w.message.contains("büyük")));
    }
}
