//! Startfenster: Vorschau, Kopie unter neuem Namen und „Pfad suchen …“
//! für die zuletzt verwendeten Projekte.

use std::path::Path;

use proava_domain::{Money, Project, ProjectName};

use super::Launcher;
use crate::settings::RecentProject;

/// What the start screen shows next to the list for the chosen project.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectPreview {
    /// Projektbezeichnung (may be empty).
    pub label: String,
    /// Bauherr (first name line, may be empty).
    pub owner: String,
    /// LV title (with number) and net total.
    pub lvs: Vec<(String, Money)>,
    pub bids: usize,
    /// Last change of the file, ISO time.
    pub modified: Option<String>,
}

impl ProjectPreview {
    pub fn of(project: &Project, modified: Option<String>) -> Self {
        let lvs = project
            .lvs()
            .map(|lv| {
                let title = if lv.number().trim().is_empty() {
                    lv.title().to_string()
                } else {
                    format!("{} {}", lv.number().trim(), lv.title())
                };
                (title, lv.net_total())
            })
            .collect();
        let bids = (0..project.lv_count())
            .map(|index| project.bids_of(index).len())
            .sum();
        Self {
            label: project.info().label.clone(),
            owner: project.info().owner.name1.clone(),
            lvs,
            bids,
            modified,
        }
    }
}

impl Launcher {
    /// Reads a project for the preview without touching the recent list.
    pub fn preview(&self, path: &Path) -> Result<ProjectPreview, String> {
        let project = self.projects.load_project(path)?;
        Ok(ProjectPreview::of(
            &project,
            self.projects.modified_iso(path),
        ))
    }

    /// „Kopie anlegen …“: the whole project (with bids, contract …) under a
    /// new name in `<folder>/<Name>/<Name>.proava`; the copy goes to the top
    /// of the recent list.
    pub fn copy_project(
        &self,
        source: &Path,
        folder: &Path,
        raw_name: &str,
    ) -> Result<RecentProject, String> {
        let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
        let project = self.projects.load_project(source)?.with_name(name);
        self.create_project_with(folder, &project)
    }

    /// „Pfad suchen …“ for a project whose file is gone: the entry now points
    /// to `found` (opened once to check it).
    pub fn relocate(&self, missing: &Path, found: &Path) -> Result<RecentProject, String> {
        let entry = self.open_project(found)?;
        if missing != found {
            self.forget(missing);
        }
        Ok(entry)
    }
}
