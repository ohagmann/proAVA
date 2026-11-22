//! Projektvorlagen: neue Projekte aus einer Vorlage, „Als Vorlage speichern …“.
//!
//! Eigene Vorlagen sind normale proAVA-Projekte im Vorlagenordner
//! (Einstellungen ▸ Neue Projekte; Vorgabe `proAVA-Vorlagen` im
//! Home-Ordner, neben der Bibliothek). Dazu kommen „leer“ und die
//! mitgelieferte GA-Vorlage nach DIN 276.

use std::path::{Path, PathBuf};

use proava_domain::template::{self, GA_TEMPLATE_NAME};
use proava_domain::{Address, CostEstimate, CostStage, OzMask, Project, ProjectName};

use super::Launcher;
use crate::settings::RecentProject;

/// Name of the default template folder in the home folder.
pub const TEMPLATE_FOLDER_NAME: &str = "proAVA-Vorlagen";

/// Where a new project comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateSource {
    /// An empty project with the defaults from the settings.
    Empty,
    /// The GA template that comes with proAVA.
    BuiltIn,
    /// The GA template with a Kostenschätzung (DIN 276, 2. Ebene) or
    /// Kostenberechnung (3. Ebene) after the templates of the Reiter Kosten.
    Costs(CostStage),
    /// An own template (a proAVA project file).
    File(PathBuf),
}

impl TemplateSource {
    /// Whether „Neues Projekt“ lets the user choose the OZ-Maske: for „leer“
    /// and the GA template. An own template brings its outline (and with it
    /// the mask of each LV) along.
    pub fn mask_changeable(&self) -> bool {
        !matches!(self, TemplateSource::File(_))
    }
}

/// One choice in „Vorlage“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateEntry {
    pub name: String,
    pub source: TemplateSource,
}

impl Launcher {
    /// Folder of the own templates.
    pub fn template_folder(&self) -> PathBuf {
        self.preferences()
            .template_folder
            .unwrap_or_else(|| self.home_folder().join(TEMPLATE_FOLDER_NAME))
    }

    /// „leer“, the GA template and the own templates (sorted by name):
    /// `<Ordner>/<Name>.proava` and `<Ordner>/<Name>/<Name>.proava`.
    pub fn templates(&self) -> Vec<TemplateEntry> {
        let mut entries = vec![
            TemplateEntry {
                name: "leer".to_string(),
                source: TemplateSource::Empty,
            },
            TemplateEntry {
                name: GA_TEMPLATE_NAME.to_string(),
                source: TemplateSource::BuiltIn,
            },
        ];
        entries.extend(CostStage::ALL.map(|stage| TemplateEntry {
            name: stage.template_name().to_string(),
            source: TemplateSource::Costs(stage),
        }));
        let folder = self.template_folder();
        if !self.folders.is_folder(&folder) {
            return entries;
        }
        let mut own: Vec<TemplateEntry> = Vec::new();
        let mut take = |file: PathBuf| {
            let Some(name) = project_stem(&file) else {
                return;
            };
            if !own.iter().any(|entry| entry.name == name) {
                own.push(TemplateEntry {
                    name,
                    source: TemplateSource::File(file),
                });
            }
        };
        for file in self.folders.files(&folder) {
            take(file);
        }
        for sub in self.folders.subfolders(&folder) {
            let Some(sub_name) = sub
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
            else {
                continue;
            };
            let file = sub.join(format!("{sub_name}.proava"));
            if self.folders.is_file(&file) {
                take(file);
            }
        }
        own.sort_by_key(|entry| entry.name.to_lowercase());
        entries.extend(own);
        entries
    }

    /// The template chosen for the last new project (name), offered first.
    pub fn last_template(&self) -> Option<String> {
        self.settings.borrow().last_template.clone()
    }

    pub fn set_last_template(&self, name: &str) {
        let name = (name != "leer").then(|| name.to_string());
        if self.last_template() != name {
            self.change_settings(|settings| settings.last_template = name);
        }
    }

    /// The OZ-Maske a new project from `source` starts with: the settings
    /// for „leer“, the template's own mask for the GA template
    /// ([`template::GA_TEMPLATE_MASK`]) and for an own template (its LV).
    pub fn template_mask(&self, source: &TemplateSource) -> Result<String, String> {
        match source {
            TemplateSource::Empty => Ok(self.empty_lv().oz_mask().pattern().to_string()),
            TemplateSource::BuiltIn | TemplateSource::Costs(_) => {
                Ok(template::GA_TEMPLATE_MASK.to_string())
            }
            TemplateSource::File(file) => self
                .projects
                .load_project(file)
                .map(|project| project.lv().oz_mask().pattern().to_string()),
        }
    }

    /// The template as a project (`None` for „leer“).
    pub fn template_project(&self, source: &TemplateSource) -> Result<Option<Project>, String> {
        self.template_project_with_mask(source, None)
    }

    /// Like [`Launcher::template_project`]; `mask` replaces the mask of the
    /// GA template (an own template keeps its own, see
    /// [`TemplateSource::mask_changeable`]).
    pub fn template_project_with_mask(
        &self,
        source: &TemplateSource,
        mask: Option<&OzMask>,
    ) -> Result<Option<Project>, String> {
        match source {
            TemplateSource::Empty => Ok(None),
            TemplateSource::BuiltIn | TemplateSource::Costs(_) => {
                let preferences = self.preferences();
                let mask = mask.cloned().unwrap_or_else(template::ga_template_mask);
                let mut project = template::ga_template(
                    mask,
                    preferences.new_lv_step,
                    preferences.new_lv_vat,
                    &self.now_iso(),
                );
                if let TemplateSource::Costs(stage) = source {
                    // Stand heute, Bearbeiter aus den Einstellungen (Drucken).
                    let today = german_date(&self.now_iso());
                    project.add_cost_estimate(CostEstimate::template(
                        *stage,
                        &today,
                        &preferences.print_author,
                    ));
                }
                Ok(Some(project))
            }
            TemplateSource::File(file) => self.projects.load_project(file).map(Some),
        }
    }

    /// New project from a template; „leer“ works like [`Launcher::create_project`].
    /// `keep_positions` takes the positions of the template along.
    pub fn create_project_from_template(
        &self,
        folder: &Path,
        raw_name: &str,
        source: &TemplateSource,
        keep_positions: bool,
    ) -> Result<RecentProject, String> {
        self.create_project_from_template_with_mask(folder, raw_name, source, keep_positions, None)
    }

    /// Like [`Launcher::create_project_from_template`] with the OZ-Maske of
    /// the dialog: `mask` (text as typed) replaces the mask from the
    /// settings („leer“) or of the GA template. An own template keeps its
    /// mask; another one is refused.
    pub fn create_project_from_template_with_mask(
        &self,
        folder: &Path,
        raw_name: &str,
        source: &TemplateSource,
        keep_positions: bool,
        mask: Option<&str>,
    ) -> Result<RecentProject, String> {
        let mask = mask
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| OzMask::parse(text).map_err(|problem| format!("OZ-Maske: {problem}")))
            .transpose()?;
        let Some(template) = self.template_project_with_mask(source, mask.as_ref())? else {
            let Some(mask) = mask else {
                return self.create_project(folder, raw_name);
            };
            let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
            let mut project = Project::new(name, self.now_iso(), self.empty_lv());
            self.apply_new_project_defaults(&mut project);
            project.lv_mut().set_oz_mask(mask);
            return self.create_project_with(folder, &project);
        };
        if let (Some(mask), TemplateSource::File(_)) = (&mask, source) {
            if mask != template.lv().oz_mask() {
                return Err(format!(
                    "Eine eigene Vorlage bringt ihre OZ-Maske mit ({}).",
                    template.lv().oz_mask().pattern()
                ));
            }
        }
        let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
        let mut project =
            template::project_from_template(&template, name, &self.now_iso(), keep_positions);
        if project.info().planner == Address::default() {
            project.info_mut().planner = self.preferences().default_planner;
        }
        self.create_project_with(folder, &project)
    }

    /// File a template of this name has (or would get) in the template folder.
    pub fn template_file(&self, raw_name: &str) -> Result<PathBuf, String> {
        let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
        Ok(self
            .template_folder()
            .join(name.as_str())
            .join(format!("{}.proava", name.as_str())))
    }

    /// „Als Vorlage speichern …“: writes the general part of `project` into
    /// the template folder (see [`template::template_from_project`]). An
    /// existing template of that name is replaced only with `replace`.
    pub fn save_as_template(
        &self,
        project: &Project,
        raw_name: &str,
        keep_positions: bool,
        replace: bool,
    ) -> Result<PathBuf, String> {
        let name = ProjectName::parse(raw_name).map_err(|error| error.to_string())?;
        let file = self.template_file(raw_name)?;
        let exists = self.folders.is_file(&file);
        if exists && !replace {
            return Err(format!("Eine Vorlage „{}“ gibt es schon.", name.as_str()));
        }
        let template =
            template::template_from_project(project, name.clone(), &self.now_iso(), keep_positions);
        let file = if exists {
            file
        } else {
            self.projects
                .create_project(&self.template_folder(), &name)?
        };
        self.projects.save_project(&file, &template)?;
        Ok(file)
    }
}

/// „2026-11-18T10:00:00“ → „18.11.2026“.
fn german_date(iso: &str) -> String {
    let date = iso.get(..10).unwrap_or(iso);
    let parts: Vec<&str> = date.split('-').collect();
    match parts.as_slice() {
        [year, month, day] => format!("{day}.{month}.{year}"),
        _ => String::new(),
    }
}

/// `Name` of `…/Name.proava`; autosave and hidden files are skipped.
fn project_stem(file: &Path) -> Option<String> {
    let name = file.file_name()?.to_string_lossy().to_string();
    if name.starts_with('.') || !name.to_lowercase().ends_with(".proava") {
        return None;
    }
    file.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
}
