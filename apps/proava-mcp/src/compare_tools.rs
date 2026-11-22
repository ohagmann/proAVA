//! `lv_vergleichen`: the LV against another state (another LV of the
//! project, another project or a GAEB file) – Änderungsliste.

use std::path::Path;

use proava_domain::lv_diff::{compare, ChangeKind};
use proava_domain::ProjectName;

use crate::args::Args;
use crate::server::{Server, PROJECT_EXTENSION};

pub(crate) fn compare_lv(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let current = loaded.project.lv();
    let other_path = args
        .opt_text("vergleich")?
        .filter(|text| !text.trim().is_empty());
    let other = match other_path {
        Some(path) => {
            let path = Path::new(path.trim());
            let mut project = if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(PROJECT_EXTENSION))
            {
                server.open_path(path)?.project
            } else {
                let file = server.allowed(path)?;
                if !file.is_file() {
                    return Err(format!("Datei nicht gefunden: {}", file.display()));
                }
                let name = ProjectName::parse("Vergleich").map_err(|error| error.to_string())?;
                server
                    .exchange()
                    .import_lv(&file, &name, "2026-01-01T00:00:00")?
                    .0
            };
            let index = crate::lv_tools::select_lv(&project, args.get("vergleich_lv"))?;
            project.set_active_lv(index);
            project
        }
        None => {
            if args.get("vergleich_lv").is_none() {
                return Err("Bitte „vergleich“ (Datei) oder „vergleich_lv“ (anderes LV dieses Projekts) angeben.".to_string());
            }
            let mut project = loaded.project.clone();
            let index = crate::lv_tools::select_lv(&project, args.get("vergleich_lv"))?;
            project.set_active_lv(index);
            project
        }
    };
    let diff = compare(other.lv(), current);
    let only: Option<ChangeKind> =
        match args.opt_text("nur")?.map(|text| text.trim().to_lowercase()) {
            None => None,
            Some(text) if text.is_empty() => None,
            Some(text) => Some(match text.as_str() {
                "neu" => ChangeKind::Added,
                "entfallen" => ChangeKind::Removed,
                "verschoben" => ChangeKind::Moved,
                "geändert" | "geaendert" => ChangeKind::Changed,
                other => {
                    return Err(format!(
                        "nur: „{other}“ – neu, entfallen, verschoben oder geändert."
                    ))
                }
            }),
        };
    let mut lines = vec![
        format!(
            "Änderungen von „{}“ ({}) gegenüber „{}“: {}",
            current.title(),
            loaded.path.display(),
            other.lv().title(),
            diff.summary()
        ),
        "Art | Zeile | OZ | Kurztext | Unterschiede".to_string(),
    ];
    lines.extend(
        diff.changes
            .iter()
            .filter(|change| only.is_none_or(|kind| change.kind == kind))
            .map(|change| change.line()),
    );
    Ok(lines.join("\n"))
}
