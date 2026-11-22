//! `projekt_anlegen` mit `vorlage`: neues Projekt aus der mitgelieferten
//! GA-Vorlage, einer eigenen Vorlage aus dem Vorlagenordner von proAVA oder
//! einer Vorlagendatei.

use std::path::{Path, PathBuf};

use proava_domain::template::{self, GA_TEMPLATE_NAME};
use proava_domain::{Address, CostEstimate, CostStage, OzMask, Project, ProjectName};

use crate::server::Server;

/// `oz_maske`, `schrittweite` and `lv_titel` of the call.
pub(crate) type Overrides<'a> = (Option<OzMask>, Option<u32>, Option<&'a str>);

pub(crate) fn create_from_template(
    server: &mut Server,
    folder: &Path,
    name: ProjectName,
    wanted: &str,
    keep_positions: bool,
    (mask, step, title): Overrides<'_>,
) -> Result<String, String> {
    let defaults = server.template_defaults().clone();
    let preferences = &defaults.preferences;
    let costs = cost_stage_of(wanted);
    let built_in = costs.is_some() || is_built_in(wanted);
    let (template, from) = if built_in {
        // Die GA-Vorlage bringt ihre Maske mit (11.222.PPPPI → 01.481);
        // oz_maske ersetzt sie, die Einstellungen nicht.
        let mask = mask.unwrap_or_else(template::ga_template_mask);
        let mut template = template::ga_template(
            mask,
            step.unwrap_or(preferences.new_lv_step),
            preferences.new_lv_vat,
            &now_iso(),
        );
        match costs {
            // GA-Gliederung plus Kostenermittlung nach der Vorlage des Reiters Kosten.
            Some(stage) => {
                let today = chrono::Local::now().format("%d.%m.%Y").to_string();
                template.add_cost_estimate(CostEstimate::template(
                    stage,
                    &today,
                    &preferences.print_author,
                ));
                (template, stage.template_name().to_string())
            }
            None => (template, GA_TEMPLATE_NAME.to_string()),
        }
    } else {
        if mask.is_some() || step.is_some() {
            return Err(
                "oz_maske und schrittweite gehen nur ohne Vorlage oder mit der GA-Vorlage – \
                 eine eigene Vorlage bringt ihre Gliederung mit."
                    .to_string(),
            );
        }
        let file = find_template(server, defaults.folder.as_deref(), wanted)?;
        let template = server.repository().load_project(&file)?;
        (template, file.display().to_string())
    };

    let mut project =
        template::project_from_template(&template, name.clone(), &now_iso(), keep_positions);
    if project.info().planner == Address::default() {
        project.info_mut().planner = preferences.default_planner.clone();
    }
    if let Some(title) = title {
        project.lv_mut().set_title(title.trim());
    }
    let path = server.repository().create_project(folder, &name)?;
    server.save_new(&path, &project)?;
    Ok(describe(&path, &project, &from))
}

fn describe(path: &Path, project: &Project, from: &str) -> String {
    let lv = project.lv();
    format!(
        "Projekt angelegt: {}\nVorlage: {from}\nLV „{}“ · OZ-Maske {} · Schrittweite {} · {}.",
        path.display(),
        lv.title(),
        lv.oz_mask().pattern(),
        lv.position_step(),
        template::summary(project).describe()
    )
}

/// „Kostenschätzung DIN 276 (2. Ebene)“, „kostenschaetzung“, „KS“ … → its stage.
fn cost_stage_of(wanted: &str) -> Option<CostStage> {
    let lower = wanted.trim().to_lowercase();
    CostStage::ALL.into_iter().find(|stage| {
        lower == stage.template_name().to_lowercase()
            || CostStage::from_key(&lower) == Some(*stage)
            || lower.starts_with(&stage.label().to_lowercase())
    })
}

fn is_built_in(wanted: &str) -> bool {
    let lower = wanted.to_lowercase();
    lower == "ga" || lower == GA_TEMPLATE_NAME.to_lowercase() || lower.contains("din 276")
}

/// A path to a template file, or the name of a template in the folder:
/// `<Ordner>/<Name>.proava` or `<Ordner>/<Name>/<Name>.proava`.
fn find_template(server: &Server, folder: Option<&Path>, wanted: &str) -> Result<PathBuf, String> {
    let as_path = Path::new(wanted);
    if as_path.is_absolute() {
        let file = server.allowed(as_path)?;
        return if file.is_file() {
            Ok(file)
        } else {
            Err(format!("Vorlage nicht gefunden: {}", file.display()))
        };
    }
    let known = folder.map(own_templates).unwrap_or_default();
    known
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
        .map(|(_, file)| file.clone())
        .ok_or_else(|| {
            let mut names = vec!["leer".to_string(), "GA".to_string()];
            names.extend(known.iter().map(|(name, _)| name.clone()));
            format!(
                "Keine Vorlage „{wanted}“. Vorhanden: {}{}",
                names.join(", "),
                folder
                    .map(|folder| format!(" (Vorlagenordner {})", folder.display()))
                    .unwrap_or_default()
            )
        })
}

/// Own templates in the folder, sorted by name.
fn own_templates(folder: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let candidate = if path.is_dir() {
            let Some(name) = path.file_name() else {
                continue;
            };
            path.join(format!("{}.proava", name.to_string_lossy()))
        } else {
            path
        };
        let Some(file_name) = candidate
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            continue;
        };
        if file_name.starts_with('.')
            || !file_name.to_lowercase().ends_with(".proava")
            || !candidate.is_file()
        {
            continue;
        }
        let stem = file_name[..file_name.len() - ".proava".len()].to_string();
        if !found.iter().any(|(known, _)| *known == stem) {
            found.push((stem, candidate));
        }
    }
    found.sort_by_key(|(name, _)| name.to_lowercase());
    found
}

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
