//! Positionsbibliotheken: the own library and further ones from the settings
//! (Büro, Netzlaufwerk – usually read-only), loading them, and a fresh
//! library. A library is an ordinary proAVA project.

use std::path::{Path, PathBuf};

use proava_domain::{Lv, OzMask, Project, ProjectName};

use crate::Launcher;

/// A library collects titles of many LVs: it gets four title levels (a
/// Kostengruppe like 481 fits), whatever the mask of new projects is.
pub const LIBRARY_MASK: &str = "11.222.33.44.PPPP";

/// One library the user can search and take from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySource {
    pub path: PathBuf,
    /// Name in lists: „Eigene Bibliothek“, „Büro“ …
    pub label: String,
    /// Nothing is stored there.
    pub read_only: bool,
    /// The own library (target of „In Bibliothek übernehmen“).
    pub own: bool,
}

impl LibrarySource {
    /// Name that positions taken from here remember (the file name without
    /// extension – it stays when the file moves to another folder).
    pub fn name(&self) -> String {
        library_name(&self.path)
    }
}

/// Name of the library at `path` for [`proava_domain::library::LibraryOrigin`].
pub fn library_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "Bibliothek".to_string())
}

/// An empty library (one LV with four title levels).
pub fn new_library(now: &str) -> Project {
    let mask = OzMask::parse(LIBRARY_MASK).expect("valid mask");
    Project::new(
        ProjectName::parse("Bibliothek").expect("valid name"),
        now,
        Lv::new("Positionsbibliothek", mask),
    )
}

impl Launcher {
    /// The own library first, then the further ones of the settings.
    pub fn libraries(&self) -> Vec<LibrarySource> {
        let own = self.library_file();
        let mut list = vec![LibrarySource {
            path: own.clone(),
            label: "Eigene Bibliothek".to_string(),
            read_only: false,
            own: true,
        }];
        for setting in self.preferences().libraries {
            if setting.path == own || list.iter().any(|known| known.path == setting.path) {
                continue;
            }
            let label = if setting.label.trim().is_empty() {
                library_name(&setting.path)
            } else {
                setting.label.trim().to_string()
            };
            list.push(LibrarySource {
                path: setting.path,
                label,
                read_only: setting.read_only,
                own: false,
            });
        }
        list
    }

    /// Reads a library. The own one may not exist yet: then it is empty
    /// (created on the first „In Bibliothek übernehmen“).
    pub fn read_library(&self, source: &LibrarySource) -> Result<Project, String> {
        if !self.is_file(&source.path) {
            return if source.own {
                Ok(new_library(&self.now_iso()))
            } else {
                Err(format!(
                    "Die Bibliothek „{}“ gibt es nicht ({}).",
                    source.label,
                    source.path.display()
                ))
            };
        }
        self.read_source(&source.path).map(|(project, _)| project)
    }

    /// The library at `path`, if it is one of the configured ones.
    pub fn library_at(&self, path: &Path) -> Option<LibrarySource> {
        self.libraries()
            .into_iter()
            .find(|source| source.path == path)
    }

    /// Whether the side window „Bibliothek“ is shown in the LV.
    pub fn library_panel(&self) -> bool {
        self.settings_value(|settings| settings.library_panel)
    }

    pub fn set_library_panel(&self, visible: bool) {
        self.change(|settings| settings.library_panel = visible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_library_has_one_empty_lv_with_four_title_levels() {
        let library = new_library("2026-11-17");
        assert_eq!(library.lv().rows().len(), 0);
        assert_eq!(library.lv().level_count(), 4);
        assert_eq!(
            library_name(Path::new("/x/Büro-Bibliothek.proava")),
            "Büro-Bibliothek"
        );
    }
}
