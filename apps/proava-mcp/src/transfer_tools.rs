//! `positionen_uebernehmen` (rows from another project, a GAEB file or the
//! own library into an LV) and `bibliothek_lesen`.

use std::path::{Path, PathBuf};

use proava_domain::transfer::{self, TransferOptions};
use proava_domain::{Lv, LvRow, Project, ProjectName, RowId};

use crate::args::{content_index, find_by_ordinal, Args};
use crate::format::row_name;
use crate::server::{Server, PROJECT_EXTENSION};

/// The project, library or GAEB LV `path` (only read), its LV `lv` active.
pub(crate) fn source_project(
    server: &Server,
    path: &Path,
    lv: Option<&serde_json::Value>,
) -> Result<Project, String> {
    let is_project = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PROJECT_EXTENSION));
    let mut project = if is_project {
        server.open_path(path)?.project
    } else {
        let file = server.allowed(path)?;
        if !file.is_file() {
            return Err(format!("Datei nicht gefunden: {}", file.display()));
        }
        // Same reader as lv_suchen (also a GAEB file in a zip), so its ids fit.
        let name = ProjectName::parse("Quelle").map_err(|error| error.to_string())?;
        server
            .exchange()
            .read_lv(&file, &name, "2026-01-01T00:00:00")?
            .project
    };
    let index = crate::lv_tools::select_lv(&project, lv)?;
    project.set_active_lv(index);
    Ok(project)
}

pub(crate) fn take_over(server: &mut Server, args: &Args) -> Result<String, String> {
    let source_path = PathBuf::from(args.text("quelle")?.trim());
    let source = source_project(server, &source_path, args.get("quelle_lv"))?;
    let wanted: Vec<String> = match args.get("oz") {
        Some(serde_json::Value::Array(list)) => list
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(text)) => vec![text.clone()],
        None | Some(serde_json::Value::Null) => Vec::new(),
        _ => {
            return Err(
                "oz: bitte die OZ der Titel/Positionen der Quelle als Liste angeben.".to_string(),
            )
        }
    };
    let wanted_ids = row_ids(args)?;
    if wanted.is_empty() && wanted_ids.is_empty() {
        return Err(
            "Bitte oz (Liste der OZ) oder ids (Liste der Zeilen-Ids, #… aus lv_suchen) angeben."
                .to_string(),
        );
    }
    let source_lv = source.lv();
    let mut ids: Vec<RowId> = wanted
        .iter()
        .map(|oz| find_by_ordinal(source_lv, oz).map(|index| source_lv.rows()[index].id()))
        .collect::<Result<_, _>>()?;
    let unknown: Vec<String> = wanted_ids
        .iter()
        .filter(|id| source_lv.index_of(**id).is_none())
        .map(|id| format!("#{}", id.0))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "ids: {} gibt es im LV „{}“ der Quelle nicht (anderes LV? quelle_lv angeben).",
            unknown.join(", "),
            source_lv.title()
        ));
    }
    for id in wanted_ids {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let options = TransferOptions {
        quantities: args.opt_flag("mengen")?.unwrap_or(true),
        prices: args.opt_flag("preise")?.unwrap_or(false),
    };
    let taken = transfer::collect(&source, source_lv, &ids, options);

    let mut loaded = server.open(args)?;
    let title = args
        .opt_text("unter_titel")?
        .filter(|text| !text.trim().is_empty() && !args.has("nach_oz") && !args.has("nach_id"));
    let range = match title {
        // Titles from the source become subtitles of this title.
        Some(title) => {
            let index = find_by_ordinal(loaded.project.lv(), title)?;
            transfer::insert_under(&mut loaded.project, index, &taken)?
        }
        None => {
            let index = content_index(loaded.project.lv(), args)?;
            transfer::insert(&mut loaded.project, index.checked_sub(1), &taken)?
        }
    };
    let lv = loaded.project.lv();
    let mut lines = vec![format!(
        "{} Zeile(n) übernommen ({} Positionen) aus {} – Status offen, Mengen {}, Preise {}:",
        range.len(),
        taken.position_count(),
        source_path.display(),
        if options.quantities {
            "übernommen"
        } else {
            "geleert"
        },
        if options.prices {
            "übernommen"
        } else {
            "geleert"
        },
    )];
    for row in &lv.rows()[range.clone()] {
        lines.push(format!("  {} {}", row.ordinal(), row_name(row)));
    }
    let answer = lines.join("\n");
    server.save_with_answer(&loaded, answer)
}

/// `ids` as row ids (integers or „#12“).
fn row_ids(args: &Args) -> Result<Vec<RowId>, String> {
    let values = match args.get("ids") {
        None | Some(serde_json::Value::Null) => return Ok(Vec::new()),
        Some(serde_json::Value::Array(list)) => list.clone(),
        Some(other) => vec![other.clone()],
    };
    values
        .iter()
        .map(|value| {
            let number = match value {
                serde_json::Value::Number(number) => number.as_u64(),
                serde_json::Value::String(text) => text.trim().trim_start_matches('#').parse().ok(),
                _ => None,
            };
            number
                .filter(|number| *number > 0)
                .map(RowId)
                .ok_or_else(|| {
                    format!("ids: „{value}“ ist keine Zeilen-Id (Zahl wie 12 oder „#12“).")
                })
        })
        .collect()
}

pub(crate) fn read_library(server: &mut Server, args: &Args) -> Result<String, String> {
    let path = args
        .opt_text("bibliothek")?
        .filter(|text| !text.trim().is_empty())
        .map(|text| PathBuf::from(text.trim()))
        .unwrap_or_else(|| server.own_library());
    let project = server.open_path(&path)?.project;
    let needle = args
        .opt_text("suche")?
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let limit = args.opt_count("max_zeilen")?.unwrap_or(300).max(1) as usize;
    let lv = project.lv();
    let rows = matching(lv, &needle);
    let mut lines = vec![format!(
        "Bibliothek {} · {} Titel, {} Positionen{} – mit positionen_uebernehmen (quelle = diese Datei, oz = …) einfügen.",
        path.display(),
        lv.group_count(),
        lv.position_count(),
        if needle.is_empty() { String::new() } else { format!(" · {} Treffer für „{needle}“", rows.len()) }
    )];
    lines.push("OZ | Art | Kurztext | Menge Einheit | Id".to_string());
    for &index in rows.iter().take(limit) {
        let row = &lv.rows()[index];
        let line = match row {
            LvRow::Group(group) => format!(
                "{} | T{} | {} | | #{}",
                group.ordinal(),
                group.level() + 1,
                group.short_text(),
                row.id().0
            ),
            LvRow::Position(position) => format!(
                "{} | Pos | {} | {} {} | #{}",
                position.ordinal(),
                position.short_text(),
                crate::format::quantity(position.quantity()),
                position.unit(),
                row.id().0
            ),
            LvRow::Note(note) => format!(" | Hinweis | {} | | #{}", note.title(), row.id().0),
            LvRow::Performance(performance) => {
                format!(" | AB | {} | | #{}", performance.heading(), row.id().0)
            }
            LvRow::Spacer(_) => format!(" | Abstand | | | #{}", row.id().0),
        };
        lines.push(line);
    }
    if rows.len() > limit {
        lines.push(format!(
            "… {} weitere Zeilen (max_zeilen erhöhen oder suche verfeinern)",
            rows.len() - limit
        ));
    }
    Ok(lines.join("\n"))
}

/// Rows whose OZ or text contains `needle`, with the titles above them.
fn matching(lv: &Lv, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return (0..lv.rows().len()).collect();
    }
    let mut keep = vec![false; lv.rows().len()];
    for (index, row) in lv.rows().iter().enumerate() {
        if format!("{} {}", row.ordinal(), row.short_text())
            .to_lowercase()
            .contains(needle)
        {
            keep[index] = true;
            let mut limit = usize::MAX;
            for above in (0..index).rev() {
                if let LvRow::Group(group) = &lv.rows()[above] {
                    if group.level() < limit {
                        keep[above] = true;
                        limit = group.level();
                    }
                }
            }
        }
    }
    (0..lv.rows().len()).filter(|&index| keep[index]).collect()
}
