//! Tools that only read.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use proava_domain::{
    Address, Bid, BidId, ComparisonLine, ComparisonOptions, Decimal, Discount, Lv, LvRow, Money,
    PositionKind, PriceComparison, PriceMark, Project,
};

use crate::args::{find_by_ordinal, find_row, Args};
use crate::format::{
    amount_columns, display_ordinal, headings_above, kind_key, lv_totals, money, percent,
    position_total, price, quantity, row_kind, shorten, surcharge_lines,
};
use crate::markup::render_markup;
use crate::server::{Server, PROJECT_EXTENSION};

const MAX_FOUND: usize = 200;
const MAX_DEPTH: usize = 8;
const LIST_LIMIT: usize = 30;

pub(crate) fn find_projects(server: &mut Server, args: &Args) -> Result<String, String> {
    let folder = server.allowed(Path::new(args.text("ordner")?.trim()))?;
    if !folder.is_dir() {
        return Err(format!("Ordner nicht gefunden: {}", folder.display()));
    }
    let pattern = args.opt_text("muster")?.unwrap_or("").to_lowercase();
    let with_old = args.flag("mit_alten_versionen")?;

    let mut found = Vec::new();
    collect_projects(&folder, 0, with_old, &mut found);
    found.retain(|(path, _, _)| {
        pattern.is_empty()
            || path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().to_lowercase().contains(&pattern))
    });
    found.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    if found.is_empty() {
        return Ok(format!("Keine proAVA-Projekte unter {}.", folder.display()));
    }
    let total = found.len();
    let mut lines = vec![format!("{total} Projekt(e) unter {}:", folder.display())];
    for (path, modified, size) in found.iter().take(MAX_FOUND) {
        lines.push(format!(
            "{} · geändert {} · {} KB",
            path.display(),
            date_time(*modified),
            size.div_ceil(1024)
        ));
    }
    if total > MAX_FOUND {
        lines.push(format!(
            "… {} weitere – bitte muster oder ordner enger fassen.",
            total - MAX_FOUND
        ));
    }
    Ok(lines.join("\n"))
}

fn collect_projects(
    folder: &Path,
    depth: usize,
    with_old: bool,
    found: &mut Vec<(PathBuf, SystemTime, u64)>,
) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            let old = name.contains("Alte Versionen");
            if depth < MAX_DEPTH && !name.starts_with('.') && (with_old || !old) {
                collect_projects(&path, depth + 1, with_old, found);
            }
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(PROJECT_EXTENSION))
        {
            if let Ok(metadata) = entry.metadata() {
                found.push((
                    path,
                    metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    metadata.len(),
                ));
            }
        }
    }
}

pub(crate) fn date_time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%d.%m.%Y %H:%M")
        .to_string()
}

pub(crate) fn numbering_note(lv: &Lv) -> &'static str {
    if lv.is_auto_numbered() {
        "fortlaufend (wird bei Änderungen neu nummeriert)"
    } else {
        "eigene OZ (neue Zeilen bekommen freie Zwischen-OZ)"
    }
}

pub(crate) fn read_project(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let lv = project.lv();
    let modified = fs::metadata(&loaded.path)
        .and_then(|metadata| metadata.modified())
        .map(date_time)
        .unwrap_or_default();

    let mut lines = vec![
        format!("Projekt: {}", project.name()),
        format!("Datei: {} (geändert {modified})", loaded.path.display()),
        format!("Angelegt: {}", project.created_iso()),
    ];
    lines.extend(crate::lv_tools::master_data_lines(project));
    if project.lv_count() > 1 {
        lines.push(format!(
            "LVs im Projekt: {} – {} (lvs_lesen; andere LVs mit lv: Nummer oder Position)",
            project.lv_count(),
            project
                .lvs()
                .map(crate::lv_tools::label)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines.push(format!(
        "LV{}: „{}“ · OZ-Maske {} (Beispiel {}) · Schrittweite {} · Nummerierung: {}",
        if lv.number().trim().is_empty() {
            String::new()
        } else {
            format!(" {}", lv.number().trim())
        },
        lv.title(),
        lv.oz_mask().pattern(),
        lv.oz_mask().example_with_step(lv.position_step()),
        lv.position_step(),
        numbering_note(lv)
    ));
    lines.push(format!(
        "Ebenen: {}{}{}",
        (0..lv.level_count())
            .map(|level| lv.level_label(level))
            .collect::<Vec<_>>()
            .join(", "),
        if lv.price_components().is_empty() {
            String::new()
        } else {
            format!(
                " · EP-Anteile: {}",
                lv.price_components()
                    .iter()
                    .map(|component| component.label.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
        if lv.time_label().trim().is_empty() {
            String::new()
        } else {
            format!(" · Zeiteinheit: {}", lv.time_label())
        }
    ));
    if lv.has_alphanumeric_positions() {
        lines.push("Positionsnummern mit Buchstaben: ja (z. B. „3A“)".to_string());
    }
    lines.push(format!(
        "Inhalt: {} Titel, {} Positionen, {} Hinweise, {} Bilder",
        lv.group_count(),
        lv.position_count(),
        lv.note_count(),
        project.images().len()
    ));
    lines.push(format!(
        "Summen: {} (Währung {})",
        lv_totals(lv),
        lv.currency()
    ));
    lines.push(format!(
        "Angebote: {}{}",
        project.bids().len(),
        if project.bids().is_empty() {
            ""
        } else {
            " (angebote_lesen, preisspiegel_lesen)"
        }
    ));
    let top: Vec<String> = lv
        .group_totals()
        .into_iter()
        .filter(|total| {
            lv.rows()[total.group_row]
                .as_group()
                .is_some_and(|group| group.level() == 0)
        })
        .map(|total| {
            let row = &lv.rows()[total.group_row];
            format!(
                "  {} {} – {}",
                row.ordinal(),
                shorten(&row.short_text(), 60),
                money(total.total)
            )
        })
        .collect();
    if !top.is_empty() {
        lines.push("Titel:".to_string());
        lines.extend(top);
    }
    Ok(lines.join("\n"))
}

pub(crate) fn read_lv(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let lv = loaded.project.lv();
    let start = match args
        .opt_text("ab_oz")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(oz) => find_by_ordinal(lv, oz)?,
        None => 0,
    };
    let end = match args
        .opt_text("bis_oz")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(oz) => lv.block_range(find_by_ordinal(lv, oz)?).end,
        None => lv.rows().len(),
    };
    let only_headings = args.flag("nur_titel")?;
    let with_long_text = args.flag("mit_langtext")?;
    let limit = args.opt_count("max_zeilen")?.unwrap_or(400).max(1) as usize;
    let totals: HashMap<usize, Money> = lv
        .group_totals()
        .into_iter()
        .map(|total| (total.group_row, total.total))
        .collect();

    let mut lines = vec![
        format!(
            "LV „{}“ · Maske {} · {} Titel, {} Positionen, {} Hinweise · {}",
            lv.title(),
            lv.oz_mask().pattern(),
            lv.group_count(),
            lv.position_count(),
            lv.note_count(),
            lv_totals(lv)
        ),
        "OZ | Art | Kurztext | Menge Einheit | EP | GP | Id".to_string(),
    ];
    if lv.rows().is_empty() {
        lines.push("(das LV ist leer)".to_string());
    }
    let mut shown = 0;
    // Rechengrundlage of Aufteilungszeilen: GP in brackets.
    let bases = lv.split_bases();
    for index in start..end.max(start) {
        let row = &lv.rows()[index];
        if only_headings && !row.is_group() {
            continue;
        }
        if shown == limit {
            let rest = lv.rows()[index..end]
                .iter()
                .filter(|row| !only_headings || row.is_group())
                .count();
            lines.push(format!(
                "… {rest} weitere Zeilen – mit ab_oz „{}“ weiterlesen.",
                display_ordinal(row.ordinal())
            ));
            break;
        }
        shown += 1;
        let id = row.id().0;
        match row {
            LvRow::Group(group) => lines.push(format!(
                "{} | {} | {} | |{} | {} | #{id}",
                display_ordinal(group.ordinal()),
                row_kind(row),
                group.short_text(),
                if group.lump_sum_total().is_some() {
                    " pauschal"
                } else {
                    ""
                },
                totals.get(&index).copied().unwrap_or(Money::ZERO).plain()
            )),
            LvRow::Position(position) => {
                let (amount, unit_price) = amount_columns(lv, position);
                let basis = bases.contains(&position.id());
                lines.push(format!(
                    "{} | {}{} | {} | {} | {} | {} | #{id}",
                    display_ordinal(position.ordinal()),
                    row_kind(row),
                    if basis { " Rechengrundlage" } else { "" },
                    position.short_text(),
                    amount,
                    unit_price,
                    crate::format::position_total_with(lv, position, basis)
                ));
                if with_long_text && !position.long_text().is_empty() {
                    lines.extend(indented(&render_markup(position.long_text())));
                }
            }
            LvRow::Performance(performance) => {
                lines.push(format!(
                    " | {} | {} | | | | #{id}",
                    row_kind(row),
                    shorten(&performance.title(), 80)
                ));
                if with_long_text {
                    lines.extend(indented(&render_markup(performance.long_text())));
                }
            }
            LvRow::Spacer(_) => lines.push(format!(" | Abstand | | | | | #{id}")),
            LvRow::Note(note) => {
                let kind = row_kind(row);
                if with_long_text {
                    lines.push(format!(" | {kind} | | | | | #{id}"));
                    lines.extend(indented(&render_markup(note.text())));
                } else {
                    lines.push(format!(
                        " | {kind} | {} | | | | #{id}",
                        shorten(&note.title(), 80)
                    ));
                }
            }
        }
    }
    Ok(lines.join("\n"))
}

fn indented(text: &str) -> Vec<String> {
    text.lines().map(|line| format!("    {line}")).collect()
}

/// „KG: 481 Automationseinrichtungen (vom Titel 01)“ for position_lesen.
fn cost_group_line(lv: &proava_domain::Lv, index: usize) -> String {
    match lv.effective_cost_group(index) {
        None => "KG: keine".to_string(),
        Some((code, inherited)) => {
            let label = proava_domain::cost_group_label(&code);
            let label = if label.is_empty() {
                String::new()
            } else {
                format!(" {label}")
            };
            let source = if inherited {
                headings_above(lv, index)
                    .into_iter()
                    .rev()
                    .find(|heading| {
                        lv.rows()[*heading]
                            .as_group()
                            .and_then(|group| group.cost_group())
                            .is_some()
                    })
                    .map(|heading| {
                        format!(
                            " (vom Titel {})",
                            display_ordinal(lv.rows()[heading].ordinal())
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!("KG: {code}{label}{source}")
        }
    }
}

/// „Leistungsbereich: 070 Gebäudeautomation (vom Titel 01)“ or „–“.
fn work_category_line(lv: &proava_domain::Lv, index: usize) -> String {
    match lv.effective_work_category(index) {
        None => "Leistungsbereich: –".to_string(),
        Some((code, inherited)) => {
            let source = if inherited {
                headings_above(lv, index)
                    .into_iter()
                    .rev()
                    .find(|heading| {
                        lv.rows()[*heading]
                            .as_group()
                            .and_then(|group| group.work_category())
                            .is_some()
                    })
                    .map(|heading| {
                        format!(
                            " (vom Titel {})",
                            display_ordinal(lv.rows()[heading].ordinal())
                        )
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            format!(
                "Leistungsbereich: {}{source}",
                proava_domain::work_category_text(&code)
            )
        }
    }
}

pub(crate) fn read_position(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let lv = project.lv();
    let index = find_row(lv, args, "oz", "id")?;
    let row = &lv.rows()[index];
    let path: Vec<String> = headings_above(lv, index)
        .into_iter()
        .map(|heading| {
            format!(
                "{} {}",
                lv.rows()[heading].ordinal(),
                lv.rows()[heading].short_text()
            )
        })
        .collect();
    let mut lines = Vec::new();
    match row {
        LvRow::Group(group) => {
            let total = lv
                .group_totals()
                .into_iter()
                .find(|total| total.group_row == index)
                .map(|total| total.total)
                .unwrap_or(Money::ZERO);
            let range = lv.block_range(index);
            lines.push(format!(
                "Titel {} (#{}) · Ebene {}",
                display_ordinal(group.ordinal()),
                row.id().0,
                group.level() + 1
            ));
            if !path.is_empty() {
                lines.push(format!("Unter: {}", path.join(" › ")));
            }
            lines.push(format!("Kurztext: {}", group.short_text()));
            lines.push(cost_group_line(lv, index));
            lines.push(work_category_line(lv, index));
            lines.push(format!(
                "Inhalt: {} Zeilen, davon {} Positionen · Summe {}{}",
                range.len() - 1,
                lv.rows()[range]
                    .iter()
                    .filter(|row| row.is_position())
                    .count(),
                money(total),
                match group.lump_sum_total() {
                    Some(_) => format!(
                        " (Pauschalsumme; Positionen zusammen {})",
                        money(lv.block_positions_total(index))
                    ),
                    None => String::new(),
                }
            ));
        }
        LvRow::Performance(performance) => {
            lines.push(format!("{} (#{})", performance.heading(), row.id().0));
            if !path.is_empty() {
                lines.push(format!("Unter: {}", path.join(" › ")));
            }
            let users: Vec<String> = lv
                .positions_using(performance.id())
                .into_iter()
                .filter_map(|id| lv.position_by_id(id))
                .map(|position| display_ordinal(position.ordinal()).to_string())
                .collect();
            lines.push(format!(
                "Nummer: {} · Bezeichnung: {}",
                performance.mark(),
                performance.label()
            ));
            if !performance.short_text().trim().is_empty() {
                lines.push(format!("Kurztext: {}", performance.short_text()));
            }
            lines.push(format!("Verwendet von: {}", users.join(", ")));
            lines.push("Text:".to_string());
            lines.push(render_markup(performance.long_text()));
        }
        LvRow::Spacer(_) => {
            lines.push(format!("Abstandshalter (#{})", row.id().0));
            if !path.is_empty() {
                lines.push(format!("Unter: {}", path.join(" › ")));
            }
            lines.push(
                "Leerzeile nur für die Bearbeitung: keine OZ, kein Text, kein Preis; erscheint weder im Druck noch in GAEB oder CSV."
                    .to_string(),
            );
        }
        LvRow::Note(note) => {
            lines.push(format!("{} (#{})", row_kind(row), row.id().0));
            if !path.is_empty() {
                lines.push(format!("Unter: {}", path.join(" › ")));
            }
            if !note.heading().trim().is_empty() {
                lines.push(format!("Kurztext: {}", note.heading()));
            }
            lines.push("Text:".to_string());
            lines.push(render_markup(note.text()));
        }
        LvRow::Position(position) => {
            lines.push(format!(
                "OZ {} (#{}) · {} ({}){}",
                display_ordinal(position.ordinal()),
                row.id().0,
                position.kind().label(),
                kind_key(position.kind()),
                match position.surcharge() {
                    Some(surcharge) if surcharge.split =>
                        " · Aufteilungszeile (die bezogenen Positionen zählen nicht selbst)",
                    Some(_) => " · Zuschlagsposition",
                    None if lv.split_bases().contains(&position.id()) =>
                        " · Rechengrundlage einer Aufteilung (GP zählt nicht selbst)",
                    None => "",
                }
            ));
            if !path.is_empty() {
                lines.push(format!("Unter: {}", path.join(" › ")));
            }
            lines.push(format!("Kurztext: {}", position.short_text()));
            let counted = if position.counts_in_total() {
                ""
            } else {
                " (zählt nicht zur Summe)"
            };
            let (amount, unit_price) = amount_columns(lv, position);
            lines.push(format!(
                "Menge: {amount} · EP {unit_price} · GP {}{counted}",
                position_total(lv, position)
            ));
            lines.extend(surcharge_lines(lv, position));
            lines.push(format!(
                "Pauschal: {} · Stundenlohn: {} · STLB-Platzhalter: {}",
                yes_no(position.is_lump_sum()),
                yes_no(position.is_hourly()),
                yes_no(position.is_stlb_placeholder())
            ));
            // STLB-Bau key with version and what is write-protected.
            if let Some(key) = proava_domain::stlb::key_line(position) {
                let state = if position.stlb_text_changed() {
                    "Langtext weicht von STLB-Bau ab (STLB-Bau*) – geht ohne Kennung in GAEB"
                } else if position.is_text_locked() {
                    "Langtext und Einheit schreibgeschützt (ändern über STLB-Bau oder zeile_aendern in_freie_position)"
                } else {
                    "Platzhalter"
                };
                lines.push(format!("Kennung: {key} · {state}"));
            }
            lines.push(cost_group_line(lv, index));
            if !position.takeoff().is_empty() {
                let (sum, errors) = position.takeoff_total();
                lines.push(format!(
                    "Mengenermittlung: {} Zeile(n), Summe {}{}",
                    position.takeoff().len(),
                    quantity(sum),
                    if errors.is_empty() {
                        ""
                    } else {
                        " (mit Fehlern)"
                    }
                ));
                for (number, line) in position.takeoff().iter().enumerate() {
                    lines.push(format!(
                        "  {}. {} | {} = {}",
                        number + 1,
                        line.label.trim(),
                        line.formula.trim(),
                        line.result()
                            .map(quantity)
                            .unwrap_or_else(|error| format!("Fehler: {error}"))
                    ));
                }
            }
            lines.push(work_category_line(lv, index));
            lines.push(format!(
                "Status: {}{}",
                position.status().label(),
                if position.is_locked() {
                    " (gesperrt – zum Ändern status „in Bearbeitung“ setzen)"
                } else {
                    ""
                }
            ));
            if let Some(performance) = lv.performance_of(position) {
                lines.push(format!(
                    "Ausführungsbeschreibung: {}",
                    performance.heading()
                ));
            }
            if let Some(base) = position.repeats().and_then(|id| lv.position_by_id(id)) {
                lines.push(format!(
                    "Wiederholung von Position {} „{}“",
                    display_ordinal(base.ordinal()),
                    shorten(base.short_text(), 60)
                ));
            }
            if position.is_not_applicable() {
                lines.push("entfällt: ja (GAEB NotAppl)".to_string());
            }
            for sub in position.sub_descriptions() {
                let amount = sub
                    .quantity
                    .map(|value| format!(" · {} {}", quantity(value), sub.unit))
                    .unwrap_or_default();
                lines.push(format!(
                    "Unterbeschreibung {}: {}{amount}",
                    if sub.number.is_empty() {
                        "-"
                    } else {
                        &sub.number
                    },
                    sub.short_text
                ));
                if !sub.long_text.is_empty() {
                    lines.extend(
                        crate::markup::render_markup(&sub.long_text)
                            .lines()
                            .map(|line| format!("  {line}")),
                    );
                }
            }
            if let Some(origin) = position.text_origin() {
                let mut line = format!("Textquelle: {}", origin.summary());
                if !origin.reference.is_empty() {
                    line.push_str(&format!(" · Artikelnummer {}", origin.reference));
                }
                if !origin.file.is_empty() {
                    line.push_str(&format!(" · Datei {}", origin.file));
                }
                if !origin.note.trim().is_empty() {
                    line.push_str(&format!(" · {}", origin.note.trim()));
                }
                lines.push(line);
            }
            if position.has_price_breakdown()
                || position.price_components().iter().any(Option::is_some)
            {
                let values: Vec<String> = lv
                    .price_components()
                    .iter()
                    .enumerate()
                    .map(|(index, component)| {
                        format!(
                            "{} {}",
                            component.label,
                            price(position.price_components().get(index).copied().flatten())
                        )
                    })
                    .collect();
                lines.push(format!(
                    "EP-Aufgliederung: {}{}",
                    yes_no(position.has_price_breakdown()),
                    if values.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", values.join(", "))
                    }
                ));
            }
            if let Some(hours) = position.time_quantity() {
                lines.push(format!(
                    "Zeitansatz: {} {}",
                    quantity(hours),
                    if lv.time_label().trim().is_empty() {
                        "je Einheit"
                    } else {
                        lv.time_label()
                    }
                ));
            }
            let places: Vec<(String, proava_domain::TextAnchor)> = position
                .complements()
                .iter()
                .filter_map(|complement| {
                    complement
                        .anchor
                        .map(|anchor| (complement.marker.clone(), anchor))
                })
                .collect();
            if position.long_text().is_empty() {
                lines.push("Langtext: (leer)".to_string());
            } else {
                lines.push("Langtext:".to_string());
                lines.push(crate::markup::render_markup_with_places(
                    position.long_text(),
                    &places,
                ));
            }
            for complement in position.complements() {
                lines.push(format!(
                    "Textergänzung {} ({}){}: Vortext „{}“ · Inhalt „{}“ · Nachtext „{}“",
                    complement.marker,
                    match complement.kind {
                        proava_domain::ComplementKind::Owner => "AG",
                        proava_domain::ComplementKind::Bidder => "Bieter",
                    },
                    if complement.anchor.is_none() {
                        ", nach dem Text"
                    } else {
                        ""
                    },
                    complement.caption,
                    complement.body,
                    complement.tail
                ));
            }
            if !project.bids().is_empty() {
                let prices: Vec<String> = project
                    .bids()
                    .iter()
                    .map(|bid| match bid.price(position.id()) {
                        Some(found) if found.not_offered => {
                            format!("{}: nicht angeboten", bid.name())
                        }
                        Some(found) if found.unit_price.is_some() => format!(
                            "{}: EP {} / GP {}",
                            bid.name(),
                            price(found.unit_price),
                            bid.total_for(position).plain()
                        ),
                        _ => format!("{}: ohne Preis", bid.name()),
                    })
                    .collect();
                lines.push(format!("Angebote: {}", prices.join("; ")));
            }
        }
    }
    Ok(lines.join("\n"))
}

fn yes_no(flag: bool) -> &'static str {
    if flag {
        "ja"
    } else {
        "nein"
    }
}

pub(crate) fn read_cost_groups(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let lv = project.lv();
    let report = proava_domain::cost_group_report(lv, project.bids());
    let mut lines = vec![format!(
        "Kosten nach Kostengruppen – LV „{}“ · Katalog {} ({})",
        lv.title(),
        lv.cost_catalog().name,
        lv.cost_catalog().id
    )];
    if !lv.has_cost_groups() {
        lines.push("Im LV ist noch keine Kostengruppe gesetzt (zeile_aendern mit kg, an Titeln oder Positionen).".into());
    }
    let mut head = "KG | Bezeichnung | Pos. | Planer".to_string();
    for bidder in &report.bidders {
        head.push_str(&format!(" | {bidder}"));
    }
    lines.push(head);
    for line in &report.lines {
        let code = match (&line.code, line.subtotal) {
            (Some(code), true) => format!("Summe {code}"),
            (Some(code), false) => code.clone(),
            (None, _) => "–".to_string(),
        };
        let mut text = format!(
            "{code} | {} | {} | {}",
            line.label,
            line.positions,
            money(line.planner)
        );
        for value in &line.bidders {
            text.push_str(&format!(" | {}", money(*value)));
        }
        lines.push(text);
    }
    let mut total = format!(
        "Gesamt | | {} | {}",
        lv.position_count(),
        money(report.planner_total)
    );
    for value in &report.bidder_totals {
        total.push_str(&format!(" | {}", money(*value)));
    }
    lines.push(total);
    Ok(lines.join("\n"))
}

pub(crate) fn check_lv(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    if !args.flag("alle")? {
        return Ok(check_active_lv(server, project, args)?.lines.join("\n"));
    }
    // Like Prüfen ▸ Alle LVs prüfen: every tender LV, the Kosten-LVs only
    // when the chosen LV is one (they go to no bidder).
    let mut indices = project.tender_lv_indices();
    if !project.lv_purpose(project.active_lv()).is_tender() {
        indices.extend(project.cost_lv_indices());
        indices.sort_unstable();
    }
    let mut copy = project.clone();
    let mut overview = Vec::new();
    let mut parts = Vec::new();
    for index in indices {
        if !copy.set_active_lv(index) {
            continue;
        }
        let mut name = crate::lv_tools::label(copy.lv());
        if let Some(stage) = copy.lv().purpose().cost_stage() {
            if !name.contains(stage.label()) {
                name.push_str(&format!(" ({})", stage.label()));
            }
        }
        let check = check_active_lv(server, &copy, args)?;
        overview.push(format!(
            "- {name}: {}, {}",
            proava_domain::wording::count(check.errors, "Fehler", "Fehler"),
            proava_domain::wording::count(check.hints, "Hinweis", "Hinweise")
        ));
        parts.push(format!("== {name} =="));
        parts.extend(check.lines);
    }
    let mut lines = vec![format!(
        "Alle LVs geprüft ({}), je LV gegliedert (Fehler und Hinweise der eigenen Prüfung, Fachregeln je LV darunter):",
        overview.len()
    )];
    lines.extend(overview);
    lines.extend(parts);
    Ok(lines.join("\n"))
}

/// The result of checking one LV: the answer lines and how many kinds of
/// errors and hints it found.
struct LvCheck {
    lines: Vec<String>,
    errors: usize,
    hints: usize,
}

/// The check of the active LV of `project` (lv_pruefen for one LV).
fn check_active_lv(server: &Server, project: &Project, args: &Args) -> Result<LvCheck, String> {
    let lv = project.lv();
    let mask = lv.oz_mask();
    let mut errors: Vec<(String, Vec<String>)> = Vec::new();
    let mut hints: Vec<(String, Vec<String>)> = Vec::new();

    let ordinal_list = |filter: &dyn Fn(&LvRow) -> bool| -> Vec<String> {
        lv.rows()
            .iter()
            .filter(|row| filter(row))
            .map(|row| match row.ordinal() {
                "" => format!("#{}", row.id().0),
                ordinal => ordinal.to_string(),
            })
            .collect()
    };

    if let Some(stage) = lv.purpose().cost_stage() {
        // Kosten-LV: KG of the level the Stufe asks for (DIN 276, 4.3.3/4.3.4).
        hints.push((
            format!(
                "{} (Kosten-LV): Kostengruppe fehlt oder ist gröber als die {}. Ebene",
                stage.label(),
                stage.level()
            ),
            proava_domain::cost_lv_findings(lv)
                .into_iter()
                .map(|(_, text)| text)
                .collect(),
        ));
    }
    errors.push(("Doppelte OZ".into(), lv.duplicate_ordinals()));
    errors.push((
        "Ohne OZ".into(),
        // Hinweise und Ausführungsbeschreibungen haben keine OZ.
        ordinal_list(&|row| {
            (row.is_group() || row.is_position()) && row.ordinal().trim().is_empty()
        }),
    ));
    errors.push((
        "Leerer Kurztext".into(),
        ordinal_list(&|row| {
            (row.is_group() || row.is_position()) && row.short_text().trim().is_empty()
        }),
    ));
    errors.push((
        "Position ohne Menge".into(),
        ordinal_list(&|row| {
            row.as_position()
                .is_some_and(|position| position.quantity_missing())
        }),
    ));
    errors.push((
        "Position ohne Einheit".into(),
        ordinal_list(&|row| {
            row.as_position().is_some_and(|position| {
                position.unit().trim().is_empty() && !position.is_surcharge()
            })
        }),
    ));
    errors.push((
        "Zuschlag „auf genannte Positionen“ ohne Bezug (zeile_aendern zuschlag.bezug)".into(),
        ordinal_list(&|row| {
            row.as_position()
                .and_then(|position| position.surcharge())
                .is_some_and(|surcharge| {
                    surcharge.kind == proava_domain::SurchargeKind::Listed
                        && surcharge.references.is_empty()
                })
        }),
    ));
    errors.push((
        "STLB-Platzhalter offen".into(),
        ordinal_list(&|row| {
            row.as_position()
                .is_some_and(|position| position.is_stlb_placeholder())
        }),
    ));
    hints.push((
        "Langtext weicht von STLB-Bau ab (STLB-Bau*) – geht ohne STLB-Bau-Kennung in GAEB; umwandeln (zeile_aendern in_freie_position: true) oder aus STLB-Bau aktualisieren (stlb_pruefdatei, stlb_aktualisieren)".into(),
        ordinal_list(&|row| {
            row.as_position()
                .is_some_and(|position| position.stlb_reference().is_some() && position.stlb_text_changed())
        }),
    ));
    hints.push((
        "STLB-Bau: TA-Textergänzung leer oder Zahlenwert fehlt (STLB-Bau Check)".into(),
        ordinal_list(&|row| {
            row.as_position().is_some_and(|position| {
                proava_domain::stlb::stlb_problems(position)
                    .iter()
                    .any(|problem| problem != proava_domain::stlb::STLB_CHANGED)
            })
        }),
    ));

    for problem in project.info().problems_as_delivered() {
        hints.push((problem, vec!["Projektangaben".to_string()]));
    }
    hints.push((
        "Ohne EP (nur für X82 und Kostenvoranschlag nötig)".into(),
        ordinal_list(&|row| {
            row.as_position()
                .is_some_and(|position| position.unit_price().is_none())
        }),
    ));
    hints.push((
        format!("OZ passt nicht zur Maske {}", mask.pattern()),
        ordinal_list(&|row| match row {
            LvRow::Group(group) => {
                !group.ordinal().is_empty()
                    && mask.read_group(group.ordinal()).map(|levels| levels.len())
                        != Some(group.level() + 1)
            }
            LvRow::Position(position) => {
                !position.ordinal().is_empty()
                    && mask.read_position(position.ordinal()).is_none()
                    // „3A“ where the LV allows letters in position numbers
                    && !(lv.has_alphanumeric_positions()
                        && mask.read_lettered_position(position.ordinal()).is_some())
            }
            LvRow::Note(_) | LvRow::Performance(_) | LvRow::Spacer(_) => false,
        }),
    ));
    hints.push((
        "Hinweis ohne Text (für eine Leerzeile in der Bearbeitung: abstand_einfuegen)".into(),
        ordinal_list(&LvRow::is_blank_note),
    ));
    let row_fields =
        |row: &LvRow| -> Vec<proava_domain::TextComplement> { row.complements().to_vec() };
    hints.push((
        "Bieterangabe ohne Vortext (der Bieter weiß nicht, was er eintragen soll)".into(),
        ordinal_list(&|row| {
            row_fields(row).iter().any(|complement| {
                complement.kind == proava_domain::ComplementKind::Bidder
                    && complement.caption.trim().is_empty()
                    && complement.tail.trim().is_empty()
            })
        }),
    ));
    let bidder_fields: usize = lv
        .rows()
        .iter()
        .map(|row| proava_domain::complement_text::count_kinds(&row_fields(row)).0)
        .sum();
    hints.push((
        format!("Bieterangaben (vom Bieter einzutragen): {bidder_fields}"),
        ordinal_list(&|row| proava_domain::complement_text::count_kinds(&row_fields(row)).0 > 0),
    ));
    hints.push((
        format!(
            "Kurztext länger als {} Zeichen (GAEB 90 kürzt)",
            proava_domain::input_limits::SHORT_TEXT_LIMIT
        ),
        ordinal_list(&|row| {
            (row.is_group() || row.is_position())
                && row.short_text().chars().count() > proava_domain::input_limits::SHORT_TEXT_LIMIT
        }),
    ));
    let (images, image_bytes) = project.lv_images();
    hints.push((
        format!(
            "Bilder im Langtext: {images}, zusammen {} (gehen mit gaeb_exportieren bilder: true – Standard – in GAEB DA XML, in GAEB 90 nicht)",
            proava_domain::size_text(image_bytes)
        ),
        ordinal_list(&|row| match row {
            LvRow::Position(position) => position.long_text().has_images(),
            LvRow::Note(note) => note.text().has_images(),
            LvRow::Performance(performance) => performance.long_text().has_images(),
            LvRow::Group(_) | LvRow::Spacer(_) => false,
        }),
    ));
    let empty_headings: Vec<String> = lv
        .rows()
        .iter()
        .enumerate()
        .filter(|(index, row)| {
            // A Datenpunkt-Abschnitt (GU-Fall) has a plant list instead.
            row.is_group()
                && !lv.rows()[lv.block_range(*index)].iter().any(|row| {
                    row.is_position()
                        || row
                            .as_group()
                            .is_some_and(proava_domain::LvGroup::is_point_section)
                })
        })
        .map(|(_, row)| display_ordinal(row.ordinal()).to_string())
        .collect();
    hints.push(("Titel ohne Positionen".into(), empty_headings));
    let base_groups: Vec<u32> = lv
        .positions()
        .filter_map(|position| match position.kind() {
            PositionKind::Base { group } => Some(group),
            _ => None,
        })
        .collect();
    hints.push((
        "Wahlposition ohne Grundposition derselben Wahlgruppe".into(),
        ordinal_list(&|row| {
            matches!(row.as_position().map(|position| position.kind()),
                Some(PositionKind::Alternative { group, .. }) if !base_groups.contains(&group))
        }),
    ));

    let errors: Vec<_> = errors
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .collect();
    let hints: Vec<_> = hints
        .into_iter()
        .filter(|(_, list)| !list.is_empty())
        .collect();
    let mut lines = vec![format!(
        "Prüfung „{}“: {} Positionen · Nummerierung {}",
        lv.title(),
        lv.position_count(),
        numbering_note(lv)
    )];
    if errors.is_empty() {
        lines.push("Fehler: keine – bereit für den GAEB-Export.".to_string());
    } else {
        lines.push(format!("Fehler ({}):", errors.len()));
        lines.extend(errors.iter().map(|(label, list)| finding(label, list)));
    }
    if hints.is_empty() {
        lines.push("Hinweise: keine.".to_string());
    } else {
        lines.push(format!("Hinweise ({}):", hints.len()));
        lines.extend(hints.iter().map(|(label, list)| finding(label, list)));
    }
    lines.extend(rule_lines(server, project, args)?);
    Ok(LvCheck {
        lines,
        errors: errors.len(),
        hints: hints.len(),
    })
}

/// GAEB-Fachregeln R1–R13 of the file the export would write (`art`,
/// default X83).
fn rule_lines(server: &Server, project: &Project, args: &Args) -> Result<Vec<String>, String> {
    use proava_application::{GaebExport, GaebRuleSeverity};
    const LIMIT: usize = 40;
    let kind = match args
        .opt_text("art")?
        .map(|text| text.trim().to_uppercase())
        .as_deref()
    {
        None | Some("") | Some("X83") | Some("83") => GaebExport::RequestForQuotation,
        Some("X82") | Some("82") => GaebExport::PricedLv,
        Some("X81") | Some("81") => GaebExport::Description,
        Some(other) => return Err(format!("GAEB-Art „{other}“ – bitte X83, X82 oder X81.")),
    };
    let findings = server.exchange().check_gaeb_export(project, kind);
    let count = |severity| findings.iter().filter(|f| f.severity == severity).count();
    let mut lines = vec![format!(
        "Fachregeln der {}, wie proAVA sie schreibt (R1–R13): {} Fehler · {} Warnungen · {} Hinweise",
        kind.extension(),
        count(GaebRuleSeverity::Error),
        count(GaebRuleSeverity::Warning),
        count(GaebRuleSeverity::Hint)
    )];
    for finding in findings.iter().take(LIMIT) {
        let severity = match finding.severity {
            GaebRuleSeverity::Error => "Fehler",
            GaebRuleSeverity::Warning => "Warnung",
            GaebRuleSeverity::Hint => "Hinweis",
        };
        lines.push(format!(
            "- {} {severity}: {} [{}]",
            finding.rule(),
            finding.text,
            finding.reference
        ));
    }
    if findings.len() > LIMIT {
        lines.push(format!("- … (+{})", findings.len() - LIMIT));
    }
    Ok(lines)
}

fn finding(label: &str, list: &[String]) -> String {
    let shown: Vec<&str> = list.iter().take(LIST_LIMIT).map(String::as_str).collect();
    let more = if list.len() > LIST_LIMIT {
        format!(" … (+{})", list.len() - LIST_LIMIT)
    } else {
        String::new()
    };
    format!("- {label} ({}): {}{more}", list.len(), shown.join(", "))
}

fn discount_text(bid: &Bid) -> String {
    match bid.discount() {
        Discount::None => "kein Nachlass".to_string(),
        Discount::Percent(value) => format!("Nachlass {}", percent(value)),
        Discount::Amount(value) => format!("Nachlass {}", money(value)),
    }
}

fn place(address: &Address) -> String {
    let city = format!("{} {}", address.postal_code, address.city);
    if city.trim().is_empty() {
        String::new()
    } else {
        format!(" ({})", city.trim())
    }
}

pub(crate) fn read_bids(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let lv = project.lv();
    if project.bids().is_empty() {
        return Ok("Noch keine Angebote eingelesen (angebot_einlesen).".to_string());
    }
    let wanted = args.opt_count("angebot")?.map(BidId);
    if let Some(id) = wanted {
        if !project.bids().iter().any(|bid| bid.id() == id) {
            return Err(format!("Ein Angebot #{} gibt es nicht.", id.0));
        }
    }
    let mut lines = vec![format!(
        "{} Angebot(e) · Kostenvoranschlag {}",
        project.bids().len(),
        money(lv.net_total())
    )];
    for bid in project
        .bids()
        .iter()
        .filter(|bid| wanted.is_none_or(|id| bid.id() == id))
    {
        lines.push(bid_summary(project, bid));
        if wanted.is_some() {
            let missing: Vec<String> = bid
                .missing_positions(lv)
                .iter()
                .map(|position| position.ordinal().to_string())
                .collect();
            if !missing.is_empty() {
                lines.push(finding("Ohne Preis", &missing));
            }
            let wrong: Vec<String> = bid
                .arithmetic_errors(lv)
                .iter()
                .map(|(position, stated, computed)| {
                    format!(
                        "{} angegeben {} statt {}",
                        position.ordinal(),
                        stated.plain(),
                        computed.plain()
                    )
                })
                .collect();
            if !wrong.is_empty() {
                lines.push(finding("Rechenfehler", &wrong));
            }
            // Abweichungsprotokoll with the proposals (angebot_einlesen
            // zuordnung text / zuordnungen takes them over).
            lines.extend(
                proava_application::protocol::structure_lines(project, bid, 200)
                    .into_iter()
                    .map(|line| format!("   {line}")),
            );
        }
    }
    Ok(lines.join("\n"))
}

pub(crate) fn bid_summary(project: &Project, bid: &Bid) -> String {
    let lv = project.lv();
    let net = bid.net_total(lv);
    let discount = bid.discount_amount(net);
    let after = net - discount;
    let gross = after + lv.vat(after);
    // „, Datum …“ and „, DPNo …“ (GAEB CTR/DPNo) after the file name.
    let mut dated = bid
        .bid_date()
        .map(|date| format!(", Datum {date}"))
        .unwrap_or_default();
    if !bid.exchange_number().trim().is_empty() {
        dated.push_str(&format!(", DPNo {}", bid.exchange_number().trim()));
    }
    let mut text = format!(
        "#{} {}{} – Datei {}{}{}\n   netto {} · {} = {} · netto nach Nachlass {} · brutto {} · ohne EP {} · Rechenfehler {}",
        bid.id().0,
        bid.name(),
        place(bid.bidder()),
        bid.source(),
        dated,
        if bid.is_excluded() { " · AUSGESCHLOSSEN" } else { "" },
        money(net),
        discount_text(bid),
        money(discount),
        money(after),
        money(gross),
        bid.missing_positions(lv).len(),
        bid.arithmetic_errors(lv).len()
    );
    if !bid.unknown_ordinals().is_empty() {
        text.push_str(&format!(
            "\n   unbekannte OZ: {}",
            bid.unknown_ordinals().join(", ")
        ));
    }
    if let Some(summary) = bid.structure().summary(bid.absent_positions(lv).len()) {
        let proposals = bid.structure().suggestions.len();
        text.push_str(&format!("\n   {summary}"));
        if proposals > 0 {
            text.push_str(&format!(
                " · {} (mit angebot)",
                proava_domain::wording::count(
                    proposals,
                    "Zuordnungsvorschlag",
                    "Zuordnungsvorschläge"
                )
            ));
        }
    }
    text
}

pub(crate) fn read_comparison(server: &mut Server, args: &Args) -> Result<String, String> {
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let lv = project.lv();
    let mut options = ComparisonOptions::default();
    if let Some(limit) = args.opt_number("ausreisser_prozent")? {
        options.outlier_percent = limit;
    }
    let comparison = PriceComparison::build(lv, project.bids(), options);
    if comparison.bids.is_empty() {
        return Ok("Kein Preisspiegel: keine (nicht ausgeschlossenen) Angebote.".to_string());
    }

    let mut lines = vec![
        format!(
            "Preisspiegel „{}“ – {} Angebot(e), Kostenvoranschlag netto {}",
            lv.title(),
            comparison.bids.len(),
            money(comparison.estimate_net)
        ),
        "Rang | Bieter | netto nach Nachlass | brutto | über Mindestbieter | ohne EP | Rechenfehler".to_string(),
    ];
    let mut ranked: Vec<_> = comparison.bids.iter().collect();
    ranked.sort_by_key(|summary| summary.rank);
    for summary in ranked {
        lines.push(format!(
            "{} | {} (#{}) | {} | {} | {} | {} | {}",
            summary.rank,
            summary.name,
            summary.bid.0,
            money(summary.net_after_discount),
            money(summary.gross),
            percent(summary.above_lowest_percent),
            summary.missing_prices,
            summary.arithmetic_errors
        ));
    }
    let names: Vec<&str> = comparison
        .bids
        .iter()
        .map(|summary| summary.name.as_str())
        .collect();

    let mut outliers = Vec::new();
    for line in &comparison.lines {
        if let ComparisonLine::Position(position) = line {
            let marked: Vec<String> = position
                .cells
                .iter()
                .zip(&names)
                .filter(|(cell, _)| cell.outlier)
                .map(|(cell, name)| {
                    format!(
                        "{name} {} ({}{})",
                        price(cell.unit_price),
                        if cell.deviation_percent.unwrap_or_default() > Decimal::ZERO {
                            "+"
                        } else {
                            ""
                        },
                        percent(cell.deviation_percent.unwrap_or_default())
                    )
                })
                .collect();
            if !marked.is_empty() {
                outliers.push(format!(
                    "- {} {} (Mittel {}): {}",
                    position.ordinal,
                    shorten(&position.short_text, 50),
                    price(position.mean_unit_price),
                    marked.join(", ")
                ));
            }
        }
    }
    if outliers.is_empty() {
        lines.push(format!(
            "Ausreißer (> {} vom Mittel): keine",
            percent(options.outlier_percent)
        ));
    } else {
        lines.push(format!(
            "Ausreißer (> {} vom Mittel, {}):",
            percent(options.outlier_percent),
            outliers.len()
        ));
        lines.extend(outliers.into_iter().take(100));
    }

    if args.flag("je_titel")? {
        lines.push(format!("Titel | Kostenvoranschlag | {}", names.join(" | ")));
        for line in &comparison.lines {
            if let ComparisonLine::Group(group) = line {
                let totals: Vec<String> = group.totals.iter().map(|total| total.plain()).collect();
                let lowest = group
                    .lowest
                    .map(|index| format!(" ← günstigst {}", names[index]))
                    .unwrap_or_default();
                lines.push(format!(
                    "{} {} | {} | {}{lowest}",
                    group.ordinal,
                    shorten(&group.short_text, 40),
                    group.estimate_total.plain(),
                    totals.join(" | ")
                ));
            }
        }
    }
    if args.flag("positionen")? {
        lines.push(format!(
            "OZ | Menge | EP Kostenvoranschlag | {} (EP, ↓ = niedrigster, ↑ = höchster)",
            names.join(" | ")
        ));
        for line in &comparison.lines {
            if let ComparisonLine::Position(position) = line {
                let cells: Vec<String> = position
                    .cells
                    .iter()
                    .map(|cell| {
                        let mark = match cell.mark {
                            PriceMark::Lowest => " ↓",
                            PriceMark::Highest => " ↑",
                            PriceMark::None => "",
                        };
                        format!("{}{mark}", price(cell.unit_price))
                    })
                    .collect();
                lines.push(format!(
                    "{} {} | {} {} | {} | {}",
                    position.ordinal,
                    shorten(&position.short_text, 40),
                    quantity(position.quantity),
                    position.unit,
                    price(position.estimate_unit_price),
                    cells.join(" | ")
                ));
            }
        }
    }
    Ok(lines.join("\n"))
}

/// rechtschreibung_pruefen: unknown words of the LV with count and places.
pub(crate) fn check_spelling(server: &mut Server, args: &Args) -> Result<String, String> {
    let limit = args.opt_count("vorschlaege")?.unwrap_or(20) as usize;
    let speller = server.speller()?;
    let loaded = server.open(args)?;
    let lv = loaded.project.lv();
    struct Found {
        count: usize,
        places: Vec<String>,
    }
    let mut found: std::collections::BTreeMap<String, Found> = std::collections::BTreeMap::new();
    for place in proava_application::spelling::lv_texts(lv) {
        let field = match place.field {
            proava_application::spelling::SpellField::Short => "Kurztext",
            proava_application::spelling::SpellField::Long => "Langtext",
        };
        let label = format!("{} {field}", place.label);
        for word in speller.unknown_words(&place.text) {
            let entry = found.entry(word.text).or_insert(Found {
                count: 0,
                places: Vec::new(),
            });
            entry.count += 1;
            if !entry.places.contains(&label) {
                entry.places.push(label.clone());
            }
        }
    }
    let source = speller
        .files()
        .map(|files| format!(" (Wörterbuch {})", files.dic.display()))
        .unwrap_or_default();
    if found.is_empty() {
        return Ok(format!(
            "Keine unbekannten Wörter im LV „{}“{source}.",
            lv.title()
        ));
    }
    let mut words: Vec<(String, Found)> = found.into_iter().collect();
    words.sort_by(|a, b| b.1.count.cmp(&a.1.count).then_with(|| a.0.cmp(&b.0)));
    let counted = if words.len() == 1 {
        "1 unbekanntes Wort".to_string()
    } else {
        format!("{} unbekannte Wörter", words.len())
    };
    let mut out = format!("{counted} im LV „{}“{source}:\n", lv.title());
    for (index, (word, entry)) in words.iter().enumerate() {
        let mut places = entry
            .places
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if entry.places.len() > 8 {
            places.push_str(&format!(" … und {} weitere", entry.places.len() - 8));
        }
        out.push_str(&format!("- {word} ({}×): {places}", entry.count));
        if index < limit {
            let suggestions = speller.suggest(word, 5);
            if !suggestions.is_empty() {
                out.push_str(&format!(" → {}", suggestions.join(", ")));
            }
        }
        out.push('\n');
    }
    Ok(out)
}
