//! LV-Suche over files: one project, one GAEB file or every project in a
//! folder tree – „Hole die Positionen für die Raumbediengeräte aus dem LV
//! von …“. The matching itself is the domain's (`proava_domain::lv_search`);
//! here the files are found and read: projects only with their outline and
//! texts (`ProjectRepository::load_project_texts`, no images), GAEB files
//! parsed. A file that cannot be read is skipped and named; a caller-given
//! stop (time limit, cancel) ends the run between two files.

use std::path::{Path, PathBuf};

use proava_domain::lv_search::{self, LvHit, LvQuery};
use proava_domain::{Project, ProjectName};

use crate::{ExchangeFiles, ProjectRepository};

/// How deep the folder walk goes below the given folder.
pub const MAX_DEPTH: usize = 8;
/// At most this many project files are searched in one run.
pub const MAX_FILES: usize = 3000;

/// GAEB LV phases a source may have (as for „Übernehmen“).
const GAEB_EXTENSIONS: [&str; 12] = [
    "x81", "x82", "x83", "x84", "x85", "x86", "d81", "d82", "d83", "d84", "d85", "d86",
];

/// What a source file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Project,
    /// GAEB LV (DA XML or GAEB 90), also inside a zip.
    Gaeb,
}

/// The kind of a source by its extension; None = neither.
pub fn source_kind(path: &Path) -> Option<SourceKind> {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if extension == "proava" {
        Some(SourceKind::Project)
    } else if GAEB_EXTENSIONS.contains(&extension.as_str()) || extension == "zip" {
        Some(SourceKind::Gaeb)
    } else {
        None
    }
}

/// Folders the walk leaves out: hidden ones and „Alte Versionen“ (also
/// „00- Alte Versionen“) unless `with_old`.
pub fn skipped_folder(name: &str, with_old: bool) -> bool {
    name.starts_with('.') || (!with_old && name.to_lowercase().contains("alte versionen"))
}

/// Project files (`.proava`) found below a folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FolderScan {
    /// Sorted by path.
    pub files: Vec<PathBuf>,
    /// More than [`MAX_FILES`] – the rest is left out.
    pub more: bool,
}

/// All `.proava` files below `root` (subfolders up to [`MAX_DEPTH`]),
/// without hidden and „Alte Versionen“ folders (see [`skipped_folder`]),
/// without backups, recovery copies and lock files. `allowed` can veto a
/// folder or file (e.g. the MCP server's folder limit).
pub fn project_files(
    repository: &dyn ProjectRepository,
    root: &Path,
    with_old: bool,
    allowed: &dyn Fn(&Path) -> bool,
) -> FolderScan {
    let mut scan = FolderScan::default();
    walk(repository, root, 0, with_old, allowed, &mut scan);
    scan.files.sort();
    scan
}

fn walk(
    repository: &dyn ProjectRepository,
    folder: &Path,
    depth: usize,
    with_old: bool,
    allowed: &dyn Fn(&Path) -> bool,
    scan: &mut FolderScan,
) {
    if scan.more {
        return;
    }
    let (folders, files) = repository.folder_entries(folder);
    for file in files {
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if source_kind(&file) != Some(SourceKind::Project)
            || name.starts_with('.')
            || !allowed(&file)
        {
            continue;
        }
        if scan.files.len() >= MAX_FILES {
            scan.more = true;
            return;
        }
        scan.files.push(file);
    }
    if depth >= MAX_DEPTH {
        return;
    }
    for sub in folders {
        let name = sub
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if skipped_folder(&name, with_old) || !allowed(&sub) {
            continue;
        }
        walk(repository, &sub, depth + 1, with_old, allowed, scan);
    }
}

/// A source read for searching: projects only with outline and texts,
/// GAEB files parsed (warnings of the reader come along). Do not save it
/// or take rows from it – images are missing (take rows from
/// `Launcher::read_source` / the full project).
pub fn read_for_search(
    repository: &dyn ProjectRepository,
    exchange: &dyn ExchangeFiles,
    path: &Path,
    now_iso: &str,
) -> Result<(Project, Vec<String>), String> {
    match source_kind(path) {
        Some(SourceKind::Project) => repository
            .load_project_texts(path)
            .map(|project| (project, Vec::new())),
        Some(SourceKind::Gaeb) => {
            let name = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            let name = ProjectName::parse(&name)
                .or_else(|_| ProjectName::parse("GAEB"))
                .map_err(|error| error.to_string())?;
            exchange
                .read_lv(path, &name, now_iso)
                .map(|read| (read.project, read.warnings))
        }
        None => Err(format!(
            "Weder proAVA-Projekt noch GAEB-LV (X81–X86, D81–D86): {}",
            path.display()
        )),
    }
}

/// The hits of one LV.
#[derive(Debug, Clone, PartialEq)]
pub struct LvHits {
    /// Position of the LV in the project (0 = first).
    pub lv_index: usize,
    /// LV-Nummer (may be empty).
    pub number: String,
    pub title: String,
    pub hits: Vec<LvHit>,
}

impl LvHits {
    /// „2 – Elektro“ or „Elektro“.
    pub fn label(&self) -> String {
        if self.number.trim().is_empty() {
            self.title.clone()
        } else {
            format!("{} – {}", self.number.trim(), self.title)
        }
    }
}

/// The hits in one file.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceHits {
    pub path: PathBuf,
    pub kind: SourceKind,
    /// Project name (GAEB: the file name).
    pub name: String,
    /// Only LVs with hits.
    pub lvs: Vec<LvHits>,
}

impl SourceHits {
    pub fn count(&self) -> usize {
        self.lvs.iter().map(|lv| lv.hits.len()).sum()
    }
}

/// Result of a search over files.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchOutcome {
    /// Files with hits, in the order searched.
    pub sources: Vec<SourceHits>,
    /// Files read and searched.
    pub searched: usize,
    /// Files that could not be read, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
    /// Files not reached because the run was stopped (time limit, cancel).
    pub not_reached: usize,
}

impl SearchOutcome {
    pub fn hit_count(&self) -> usize {
        self.sources.iter().map(SourceHits::count).sum()
    }

    pub fn stopped(&self) -> bool {
        self.not_reached > 0
    }
}

/// Searches the LVs of a project: only `lv` (index) or all of them.
pub fn search_project(project: &Project, lv: Option<usize>, query: &LvQuery) -> Vec<LvHits> {
    project
        .lvs()
        .enumerate()
        .filter(|(index, _)| lv.is_none_or(|wanted| wanted == *index))
        .filter_map(|(index, content)| {
            let hits = lv_search::search(content, query);
            (!hits.is_empty()).then(|| LvHits {
                lv_index: index,
                number: content.number().to_string(),
                title: content.title().to_string(),
                hits,
            })
        })
        .collect()
}

/// Searches every file in `files` (projects and GAEB, see
/// [`read_for_search`]). `stop` is asked before each file; once it says
/// yes, the rest counts as not reached.
pub fn search_files(
    repository: &dyn ProjectRepository,
    exchange: &dyn ExchangeFiles,
    files: &[PathBuf],
    query: &LvQuery,
    now_iso: &str,
    stop: &mut dyn FnMut() -> bool,
) -> SearchOutcome {
    let mut outcome = SearchOutcome::default();
    for (number, path) in files.iter().enumerate() {
        if stop() {
            outcome.not_reached = files.len() - number;
            break;
        }
        match read_for_search(repository, exchange, path, now_iso) {
            Ok((project, _)) => {
                outcome.searched += 1;
                let lvs = search_project(&project, None, query);
                if !lvs.is_empty() {
                    outcome.sources.push(SourceHits {
                        path: path.clone(),
                        kind: source_kind(path).unwrap_or(SourceKind::Project),
                        name: project.name().to_string(),
                        lvs,
                    });
                }
            }
            Err(error) => outcome.skipped.push((path.clone(), error)),
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use proava_domain::{Decimal, Lv, OzMask, Position, RichText};

    /// Projects in memory, a folder tree as a map.
    struct Memory {
        projects: HashMap<PathBuf, Project>,
        folders: HashMap<PathBuf, (Vec<PathBuf>, Vec<PathBuf>)>,
    }

    impl ProjectRepository for Memory {
        fn create_project(&self, _: &Path, _: &ProjectName) -> Result<PathBuf, String> {
            unreachable!()
        }
        fn load_project(&self, file: &Path) -> Result<Project, String> {
            self.projects
                .get(file)
                .cloned()
                .ok_or_else(|| format!("{}: beschädigt", file.display()))
        }
        fn folder_entries(&self, folder: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
            self.folders.get(folder).cloned().unwrap_or_default()
        }
        fn save_project(&self, _: &Path, _: &Project) -> Result<(), String> {
            unreachable!()
        }
        fn backup(&self, _: &Path, _: &Path) -> Result<PathBuf, String> {
            unreachable!()
        }
        fn restore(&self, _: &Path, _: &Path) -> Result<PathBuf, String> {
            unreachable!()
        }
        fn modified_iso(&self, _: &Path) -> Option<String> {
            None
        }
        fn remove_file(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
    }

    fn project(name: &str, texts: &[(&str, &str)]) -> Project {
        let mut lv = Lv::new("LV", OzMask::parse("11.22.PPPP").unwrap());
        for (short, long) in texts {
            let mut position = Position::new("", *short, Decimal::ONE, "St", None);
            position.set_long_text(RichText::plain(long));
            lv.push_position(position);
        }
        lv.renumber();
        Project::new(ProjectName::parse(name).unwrap(), "2026-11-18", lv)
    }

    fn memory() -> Memory {
        let root = PathBuf::from("/p");
        let a = root.join("A/A.proava");
        let b = root.join("B/B.proava");
        let old = root.join("A/00- Alte Versionen/A_V1.proava");
        let broken = root.join("C.proava");
        let mut projects = HashMap::new();
        projects.insert(
            a.clone(),
            project("A", &[("Raumbediengerät", "mit Display, KNX")]),
        );
        projects.insert(b.clone(), project("B", &[("Präsenzmelder", "KNX, Decke")]));
        projects.insert(old.clone(), project("A", &[("Raumbediengerät", "alt")]));
        let mut folders = HashMap::new();
        folders.insert(
            root.clone(),
            (
                vec![root.join("A"), root.join("B"), root.join(".git")],
                vec![
                    broken,
                    root.join("notiz.txt"),
                    root.join(".A.proava.autosave"),
                ],
            ),
        );
        folders.insert(
            root.join("A"),
            (
                vec![root.join("A/00- Alte Versionen")],
                vec![a, root.join("A/LV.X83")],
            ),
        );
        folders.insert(root.join("A/00- Alte Versionen"), (Vec::new(), vec![old]));
        folders.insert(root.join("B"), (Vec::new(), vec![b]));
        folders.insert(
            root.join(".git"),
            (Vec::new(), vec![root.join(".git/x.proava")]),
        );
        Memory { projects, folders }
    }

    #[test]
    fn folder_walk_leaves_out_old_versions_hidden_folders_and_other_files() {
        let memory = memory();
        let scan = project_files(&memory, Path::new("/p"), false, &|_| true);
        assert_eq!(
            scan.files,
            [
                PathBuf::from("/p/A/A.proava"),
                PathBuf::from("/p/B/B.proava"),
                PathBuf::from("/p/C.proava")
            ]
        );
        assert!(!scan.more);
        let with_old = project_files(&memory, Path::new("/p"), true, &|_| true);
        assert_eq!(with_old.files.len(), 4);
        let only_a = project_files(&memory, Path::new("/p"), false, &|path| {
            !path.starts_with("/p/B")
        });
        assert_eq!(only_a.files.len(), 2, "vetoed folder");
        assert!(skipped_folder("00- Alte Versionen", false));
        assert!(skipped_folder("Alte Versionen", false));
        assert!(!skipped_folder("Alte Versionen", true));
        assert!(skipped_folder(".sync-client", true));
    }

    #[test]
    fn files_are_searched_grouped_and_broken_ones_skipped() {
        let memory = memory();
        let files = project_files(&memory, Path::new("/p"), false, &|_| true).files;
        let query = LvQuery::text("knx");
        let outcome = search_files(
            &memory,
            &crate::NoExchange,
            &files,
            &query,
            "2026-11-18T12:00:00",
            &mut || false,
        );
        assert_eq!(outcome.searched, 2);
        assert_eq!(outcome.hit_count(), 2);
        assert_eq!(
            outcome
                .sources
                .iter()
                .map(|source| source.name.as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert_eq!(
            outcome.sources[0].lvs[0].hits[0].short_text,
            "Raumbediengerät"
        );
        assert_eq!(outcome.skipped.len(), 1, "C is broken");
        assert!(outcome.skipped[0].1.contains("beschädigt"));
        assert!(!outcome.stopped());

        // A stop after the first file.
        let mut asked = 0;
        let outcome = search_files(
            &memory,
            &crate::NoExchange,
            &files,
            &query,
            "2026-11-18T12:00:00",
            &mut || {
                asked += 1;
                asked > 1
            },
        );
        assert_eq!((outcome.searched, outcome.not_reached), (1, 2));
        assert!(outcome.stopped());
    }

    #[test]
    fn sources_by_extension() {
        assert_eq!(
            source_kind(Path::new("/a/b.PROAVA")),
            Some(SourceKind::Project)
        );
        assert_eq!(source_kind(Path::new("/a/b.X83")), Some(SourceKind::Gaeb));
        assert_eq!(source_kind(Path::new("/a/b.d86")), Some(SourceKind::Gaeb));
        assert_eq!(source_kind(Path::new("/a/b.pdf")), None);
        let error = read_for_search(
            &memory(),
            &crate::NoExchange,
            Path::new("/a/b.pdf"),
            "2026-11-18T12:00:00",
        )
        .unwrap_err();
        assert!(error.contains("Weder"), "{error}");
    }
}
