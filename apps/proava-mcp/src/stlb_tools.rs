//! `stlb_uebernehmen`: positions from a GAEB file written by STLB-Bau (or a
//! zip with one) into the LV – fills an STLB placeholder or inserts them.
//! The STLB-Bau key of each position is kept (see `docs/FACHLICHES.md`). The
//! same tool takes manufacturer texts from AUSSCHREIBEN.DE (`quelle`); each
//! position notes its Herkunft.

use std::path::Path;

use proava_domain::stlb::{adopt_catalog_cost_group, fill_from_catalog};
use proava_domain::{normalize_ordinal, EditStatus, LvRow, Position, RowKind, LOCKED_HINT};

use crate::args::{content_index, find_row, Args};
use crate::format::{ordinals_by_id, renumbered_count, row_name, sums_after_change};
use crate::server::Server;

pub(crate) fn take_over(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let file = server.allowed(Path::new(args.text("datei")?.trim()))?;
    if !file.is_file() {
        return Err(format!("Datei nicht gefunden: {}", file.display()));
    }
    let mut read = server.exchange().read_catalog_positions(&file)?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let source = match args
        .get("quelle")
        .and_then(|value| value.as_str())
        .map(str::trim)
    {
        Some(source) if !source.is_empty() => {
            proava_domain::text_origin::adopt_source(&mut read.positions, source, true, &today);
            source.to_string()
        }
        _ => {
            proava_domain::text_origin::adopt_source(
                &mut read.positions,
                "GAEB-Datei",
                false,
                &today,
            );
            if read.with_stlb_key > 0 {
                "STLB-Bau"
            } else {
                "der Datei"
            }
            .to_string()
        }
    };
    // Each entry with its kind: the suggestion of the file (a Standard-
    // beschreibung without unit and quantity is a text), `als` overrides it.
    let mut positions: Vec<(Position, RowKind)> = read
        .positions
        .iter()
        .enumerate()
        .map(|(index, position)| (position.clone(), read.kind(index).suggested))
        .collect();
    match args.get("als") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(all)) => {
            let kind = kind_of(all)?;
            positions.iter_mut().for_each(|(_, chosen)| *chosen = kind);
        }
        Some(serde_json::Value::Object(map)) => {
            for (oz, value) in map {
                let kind = kind_of(value.as_str().unwrap_or_default())?;
                let key = ordinal_key(oz);
                let entry = positions
                    .iter_mut()
                    .find(|(position, _)| ordinal_key(position.ordinal()) == key)
                    .ok_or_else(|| format!("als: die OZ {oz} gibt es in der Datei nicht."))?;
                entry.1 = kind;
            }
        }
        Some(other) => {
            return Err(format!(
                "als: „{other}“ – eine Zeilenart für alle (position, hinweis, ausfuehrungsbeschreibung) oder {{OZ: Zeilenart}}."
            ))
        }
    }
    if let Some(serde_json::Value::Array(wanted)) = args.get("positionen") {
        let wanted: Vec<String> = wanted
            .iter()
            .filter_map(|value| value.as_str())
            .map(ordinal_key)
            .collect();
        let missing: Vec<&String> = wanted
            .iter()
            .filter(|oz| {
                !positions
                    .iter()
                    .any(|(p, _)| &ordinal_key(p.ordinal()) == *oz)
            })
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "In der Datei fehlen die OZ {}. Vorhanden: {}",
                missing
                    .iter()
                    .map(|oz| oz.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                positions
                    .iter()
                    .map(|(p, _)| p.ordinal())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        positions.retain(|(p, _)| wanted.contains(&ordinal_key(p.ordinal())));
    }
    if positions.is_empty() {
        return Err("Keine Position zu übernehmen.".to_string());
    }

    let lv = loaded.project.lv_mut();
    let before = ordinals_by_id(lv);
    let mut changed = Vec::new();
    let mut texts = 0;
    let mut rest = positions.into_iter();
    let mut cursor = if args.has("platzhalter_oz") || args.has("platzhalter_id") {
        let index = find_row(lv, args, "platzhalter_oz", "platzhalter_id")?;
        let target = lv
            .row_mut(index)
            .and_then(LvRow::as_position_mut)
            .filter(|position| position.is_stlb_placeholder())
            .ok_or("Die angegebene Zeile ist kein STLB-Bau-Platzhalter.")?;
        if target.status() == EditStatus::Done {
            return Err(format!(
                "Der Platzhalter ist fertig und damit gesperrt – {LOCKED_HINT}. Nichts gespeichert."
            ));
        }
        let (first, kind) = rest.next().ok_or("Keine Position zu übernehmen.")?;
        fill_from_catalog(target, &first);
        changed.push(target.id());
        if kind != RowKind::Position {
            lv.convert_row(index, kind)
                .map_err(|error| format!("{error} Nichts gespeichert."))?;
            texts += 1;
        }
        index + 1
    } else {
        content_index(lv, args)?
    };
    for (position, kind) in rest {
        let mut fresh = Position::new(
            "",
            position.short_text(),
            position.quantity(),
            position.unit(),
            position.unit_price(),
        );
        fill_from_catalog(&mut fresh, &position);
        let id = lv
            .insert_numbered(cursor, LvRow::Position(fresh))
            .map_err(|error| format!("{error} Nichts gespeichert."))?;
        if kind != RowKind::Position {
            let index = lv.index_of(id).unwrap_or(cursor);
            lv.convert_row(index, kind)
                .map_err(|error| format!("{error} Nichts gespeichert."))?;
            texts += 1;
        }
        cursor = lv.index_of(id).unwrap_or(cursor) + 1;
        changed.push(id);
    }

    // Kostengruppe aus der STLB-Bau-Kennung, wenn der Titel keine gleiche gibt.
    let lv = loaded.project.lv_mut();
    let mut with_cost_group = 0;
    for id in &changed {
        if let Some(index) = lv.index_of(*id) {
            with_cost_group += usize::from(adopt_catalog_cost_group(lv, index));
        }
    }
    let lv = loaded.project.lv();
    let mut lines = vec![format!(
        "{} {} aus {source} übernommen{}, Status offen ({} mit STLB-Bau-Kennung in der Datei):",
        changed.len(),
        if texts > 0 {
            "Zeile(n)"
        } else {
            "Position(en)"
        },
        match texts {
            0 => String::new(),
            count => format!(", davon {count} als Text (Hinweis/Ausführungsbeschreibung)"),
        },
        read.with_stlb_key
    )];
    for id in &changed {
        if let Some(row) = lv.row_by_id(*id) {
            let mut line = format!("- {}", row_name(row));
            let catalog = match row {
                LvRow::Position(position) => position.catalog_line(),
                LvRow::Note(note) => note.catalog_line(),
                LvRow::Performance(performance) => performance.catalog().print_line(),
                _ => None,
            };
            if let Some(catalog) = catalog {
                line.push_str(&format!(" [{catalog}]"));
            }
            if let Some(origin) = row.as_position().and_then(Position::text_origin) {
                line.push_str(&format!(" (Textquelle: {})", origin.summary()));
            }
            lines.push(line);
        }
    }
    if with_cost_group > 0 {
        lines.push(format!(
            "Kostengruppe aus STLB-Bau gesetzt bei {with_cost_group} Position(en)."
        ));
    }
    if !read.source_in_archive.is_empty() {
        lines.push(format!(
            "Aus dem Archiv gelesen: {}",
            read.source_in_archive
        ));
    }
    for warning in &read.warnings {
        lines.push(format!("Hinweis: {warning}"));
    }
    let moved = renumbered_count(&before, lv, &changed);
    if moved > 0 {
        lines.push(format!("{moved} weitere Zeilen neu nummeriert."));
    }
    lines.push(sums_after_change(lv, changed.last().copied()));
    server.save_with_answer(&loaded, lines.join("\n"))
}

/// Zeilenart from `als` („position“, „hinweis“, „ausfuehrungsbeschreibung“).
fn kind_of(text: &str) -> Result<RowKind, String> {
    RowKind::parse(text).ok_or_else(|| {
        format!("als: „{text}“ – bitte position, hinweis oder ausfuehrungsbeschreibung.")
    })
}

/// OZ for comparing: leading zeros of numeric pieces do not count ("1.10" = "01.0010").
fn ordinal_key(ordinal: &str) -> String {
    normalize_ordinal(ordinal)
        .split('.')
        .map(|piece| {
            if piece.chars().all(|ch| ch.is_ascii_digit()) {
                let trimmed = piece.trim_start_matches('0');
                if trimmed.is_empty() { "0" } else { trimmed }.to_string()
            } else {
                piece.to_uppercase()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// `stlb_pruefdatei`: the STLB-Bau positions of the LV (with titles, OZ as in
/// the LV) as X81 for checking or updating in STLB-Bau.
pub(crate) fn check_file(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let reduced = proava_domain::stlb_update::positions_for_check(loaded.project.lv());
    let count = reduced.positions().count();
    if count == 0 {
        return Err("Das LV hat keine Positionen mit STLB-Bau-Kennung.".to_string());
    }
    let file = crate::exchange_tools::target(
        server,
        args,
        &loaded.path,
        format!(
            "{}_STLB-Bau_Pruefen.X81",
            crate::exchange_tools::stem(&loaded.path)
        ),
    )?;
    let mut project = loaded.project.clone();
    *project.lv_mut() = reduced;
    server
        .exchange()
        .export_gaeb(&project, proava_application::GaebExport::Description, &file)?;
    Ok(format!(
        "{count} STLB-Bau-Position(en) als X81 geschrieben: {}\nDie Datei in STLB-Bau einlesen (Favoriten bzw. Leistungsverzeichnis), prüfen oder aktualisieren, wieder als GAEB speichern und mit stlb_aktualisieren einlesen.",
        file.display()
    ))
}

/// `stlb_aktualisieren`: matches the positions of a file STLB-Bau wrote
/// after checking/updating with the LV (OZ, else Leistung) and lists the
/// differences; with `uebernehmen` the texts are taken over (OZ, quantity,
/// prices stay, status open, finished positions are left alone).
pub(crate) fn update(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let file = server.allowed(Path::new(args.text("datei")?.trim()))?;
    if !file.is_file() {
        return Err(format!("Datei nicht gefunden: {}", file.display()));
    }
    let read = server.exchange().read_catalog_positions(&file)?;
    // Only what the file delivered as positions (texts are not updated).
    let plan =
        proava_domain::stlb_update::plan_update(loaded.project.lv(), &read.delivered_positions());
    let mut lines = vec![format!(
        "{} geändert, {} unverändert, {} ohne Partner im LV.",
        plan.updates.len(),
        plan.unchanged,
        plan.unmatched.len()
    )];
    for update in &plan.updates {
        let details: Vec<String> = update.details.iter().map(|detail| detail.line()).collect();
        let mut line = format!(
            "- {} {} ({}{}): ",
            update.ordinal,
            update.short_text,
            update.matched_by.label(),
            if update.locked {
                ", fertig – bleibt"
            } else {
                ""
            }
        );
        if update.catalog.0 != update.catalog.1 {
            line.push_str(&format!(
                "Kennung {} → {}; ",
                update.catalog.0, update.catalog.1
            ));
        }
        line.push_str(&details.join("; "));
        lines.push(line.trim_end_matches("; ").to_string());
    }
    if !plan.unmatched.is_empty() {
        lines.push(format!("Ohne Partner im LV: {}", plan.unmatched.join("; ")));
    }
    for warning in &read.warnings {
        lines.push(format!("Hinweis: {warning}"));
    }
    if !args.flag("uebernehmen")? || plan.updates.is_empty() {
        lines.push("Nichts geändert (Vorschau; übernehmen mit uebernehmen: true).".to_string());
        return Ok(lines.join("\n"));
    }
    let count = proava_domain::stlb_update::apply_updates(loaded.project.lv_mut(), &plan.updates);
    lines.push(format!("{count} Position(en) aktualisiert, Status offen."));
    server.save_with_answer(&loaded, lines.join("\n"))
}
