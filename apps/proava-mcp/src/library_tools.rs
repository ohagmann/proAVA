//! Positionsbibliothek for KIs: search all libraries (full text, filters),
//! insert entries into an LV (they remember their origin), store LV rows in
//! the own library, and compare positions with their library entries.

use std::path::PathBuf;

use proava_application::library::{new_library, LibrarySource};
use proava_domain::library::{self, EntryInfo, KindFilter, SearchFilter, SearchIndex};
use proava_domain::lv_diff::{self, ChangeKind};
use proava_domain::transfer::{self, TransferOptions};
use proava_domain::{LvGroup, LvRow, Project, RowId};

use crate::args::{content_index, find_by_ordinal, Args};
use crate::format::{price, quantity};
use crate::server::Server;

/// The libraries chosen with `bibliothek` (label, file name or path; empty
/// = all), each with its project.
fn chosen(
    server: &Server,
    args: &Args,
    all_by_default: bool,
) -> Result<Vec<(LibrarySource, Project)>, String> {
    let wanted = args
        .opt_text("bibliothek")?
        .unwrap_or_default()
        .trim()
        .to_string();
    let sources = server.libraries();
    let picked: Vec<LibrarySource> = if wanted.is_empty() {
        if all_by_default {
            sources
        } else {
            sources.into_iter().take(1).collect()
        }
    } else {
        let lower = wanted.to_lowercase();
        let found: Vec<LibrarySource> = sources
            .iter()
            .filter(|source| {
                source.label.to_lowercase() == lower
                    || source.name().to_lowercase() == lower
                    || source.path.as_os_str() == wanted.as_str()
            })
            .cloned()
            .collect();
        if found.is_empty() {
            let path = PathBuf::from(&wanted);
            if path.is_absolute() {
                vec![LibrarySource {
                    label: proava_application::library::library_name(&path),
                    path,
                    read_only: false,
                    own: false,
                }]
            } else {
                return Err(format!(
                    "Bibliothek „{wanted}“ unbekannt. Eingerichtet: {}.",
                    sources
                        .iter()
                        .map(|source| source.label.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        } else {
            found
        }
    };
    let mut libraries = Vec::new();
    for source in picked {
        if !source.path.is_file() {
            if source.own {
                libraries.push((source, new_library("2026-01-01T00:00:00")));
                continue;
            }
            return Err(format!(
                "Die Bibliothek „{}“ gibt es nicht ({}).",
                source.label,
                source.path.display()
            ));
        }
        let project = server.open_path(&source.path)?.project;
        libraries.push((source, project));
    }
    Ok(libraries)
}

fn kind_filter(args: &Args) -> Result<KindFilter, String> {
    Ok(
        match args
            .opt_text("art")?
            .unwrap_or_default()
            .trim()
            .to_lowercase()
            .as_str()
        {
            "" | "alle" => KindFilter::All,
            "normal" => KindFilter::Normal,
            "bedarf" => KindFilter::Provisional,
            "wahl" | "grund" => KindFilter::Alternative,
            "zulage" => KindFilter::Supplement,
            "pauschal" => KindFilter::LumpSum,
            "stundenlohn" => KindFilter::Hourly,
            other => {
                return Err(format!(
                "art „{other}“ – bitte normal, bedarf, wahl, zulage, pauschal oder stundenlohn."
            ))
            }
        },
    )
}

fn today() -> String {
    chrono::Local::now().format("%d.%m.%Y").to_string()
}

pub(crate) fn search(server: &mut Server, args: &Args) -> Result<String, String> {
    let libraries = chosen(server, args, true)?;
    let filter = SearchFilter {
        text: args.opt_text("suche")?.unwrap_or_default().to_string(),
        cost_group: args.opt_text("kg")?.unwrap_or_default().trim().to_string(),
        kind: kind_filter(args)?,
        keyword: args
            .opt_text("stichwort")?
            .unwrap_or_default()
            .trim()
            .to_string(),
    };
    if filter.is_empty() {
        return Err(
            "Bitte suche, kg, art oder stichwort angeben (ganze Bibliothek: bibliothek_lesen)."
                .to_string(),
        );
    }
    let limit = args.opt_count("max_treffer")?.unwrap_or(50).max(1) as usize;
    let long = args.opt_flag("mit_langtext")?.unwrap_or(false);
    let mut lines = Vec::new();
    let mut total = 0;
    for (source, project) in &libraries {
        let lv = project.lv();
        let index = SearchIndex::build(lv);
        let hits: Vec<usize> = index
            .hits(lv, &filter)
            .into_iter()
            .filter(|&row| lv.rows()[row].is_position() && index.matches(lv, row, &filter))
            .collect();
        total += hits.len();
        if hits.is_empty() {
            continue;
        }
        lines.push(format!(
            "— {} ({}): {} Treffer",
            source.label,
            source.path.display(),
            hits.len()
        ));
        for &row in hits.iter().take(limit) {
            let Some(position) = lv.rows()[row].as_position() else {
                continue;
            };
            let info = position.library_info();
            let mut line = format!(
                "{} | {} | {} {} | EP {}",
                position.ordinal(),
                position.short_text(),
                quantity(position.quantity()),
                position.unit(),
                price(position.unit_price()),
            );
            if !info.price_date.trim().is_empty() {
                line.push_str(&format!(" (Preis {})", info.price_date.trim()));
            }
            if !info.as_of.trim().is_empty() {
                line.push_str(&format!(" | Stand {}", info.as_of.trim()));
            }
            if !info.keywords.is_empty() {
                line.push_str(&format!(" | Stichwörter: {}", info.keywords_text()));
            }
            line.push_str(&format!(" | #{}", position.id().0));
            lines.push(line);
            if long {
                for text in position
                    .long_text()
                    .to_plain()
                    .lines()
                    .filter(|text| !text.trim().is_empty())
                {
                    lines.push(format!("    {}", text.trim()));
                }
            }
        }
        if hits.len() > limit {
            lines.push(format!(
                "  … {} weitere (max_treffer erhöhen)",
                hits.len() - limit
            ));
        }
    }
    let mut answer = vec![format!(
        "{total} Positionen gefunden in {} Bibliothek(en). Einfügen mit bibliothek_einfuegen (bibliothek, oz oder ids).",
        libraries.len()
    )];
    answer.extend(lines);
    Ok(answer.join("\n"))
}

pub(crate) fn insert(server: &mut Server, args: &Args) -> Result<String, String> {
    let libraries = chosen(server, args, false)?;
    let Some((source, library)) = libraries.into_iter().next() else {
        return Err("Keine Bibliothek gewählt.".to_string());
    };
    let lv = library.lv();
    let mut ids: Vec<RowId> = Vec::new();
    if let Some(serde_json::Value::Array(list)) = args.get("oz") {
        for oz in list.iter().filter_map(|value| value.as_str()) {
            ids.push(lv.rows()[find_by_ordinal(lv, oz)?].id());
        }
    }
    if let Some(serde_json::Value::Array(list)) = args.get("ids") {
        for id in list.iter().filter_map(serde_json::Value::as_u64) {
            if lv.row_by_id(RowId(id)).is_none() {
                return Err(format!(
                    "#{id} gibt es in der Bibliothek „{}“ nicht.",
                    source.label
                ));
            }
            ids.push(RowId(id));
        }
    }
    if ids.is_empty() {
        return Err(
            "Bitte oz (Liste) oder ids (Liste, #… aus bibliothek_suchen) angeben.".to_string(),
        );
    }
    let options = TransferOptions {
        quantities: args.opt_flag("mengen")?.unwrap_or(false),
        prices: args.opt_flag("preise")?.unwrap_or(true),
    };
    let mut taken = transfer::collect(&library, lv, &ids, options);
    library::mark_origin(&mut taken.rows, &source.name());

    let mut loaded = server.open(args)?;
    let title = args
        .opt_text("unter_titel")?
        .filter(|text| !text.trim().is_empty() && !args.has("nach_oz") && !args.has("nach_id"));
    let range = match title {
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
        "{} Zeile(n) aus „{}“ eingefügt ({} Positionen, Status offen, Mengen {}, Erfahrungspreise {}; Herkunft gemerkt für bibliothek_abgleichen):",
        range.len(),
        source.label,
        taken.position_count(),
        if options.quantities { "übernommen" } else { "leer" },
        if options.prices { "als Planerpreise" } else { "nicht übernommen" },
    )];
    for row in &lv.rows()[range] {
        lines.push(format!(
            "  {} {}",
            row.ordinal(),
            crate::format::row_name(row)
        ));
    }
    server.save_with_answer(&loaded, lines.join("\n"))
}

pub(crate) fn store(server: &mut Server, args: &Args) -> Result<String, String> {
    // Rows of the LV.
    let loaded = server.open(args)?;
    let project = &loaded.project;
    let wanted: Vec<String> = match args.get("oz") {
        Some(serde_json::Value::Array(list)) => list
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(text)) => vec![text.clone()],
        _ => {
            return Err(
                "oz: bitte die OZ der Titel/Positionen des LV als Liste angeben.".to_string(),
            )
        }
    };
    let ids: Vec<RowId> = wanted
        .iter()
        .map(|oz| find_by_ordinal(project.lv(), oz).map(|index| project.lv().rows()[index].id()))
        .collect::<Result<_, _>>()?;
    let mut taken = transfer::collect(
        project,
        project.lv(),
        &ids,
        TransferOptions {
            quantities: args.opt_flag("mengen")?.unwrap_or(true),
            prices: args.opt_flag("preise")?.unwrap_or(true),
        },
    );
    library::prepare_for_library(&mut taken.rows, &today());
    if let Some(words) = args
        .opt_text("stichwoerter")?
        .filter(|text| !text.trim().is_empty())
    {
        let keywords = EntryInfo::parse_keywords(words);
        for row in &mut taken.rows {
            if let LvRow::Position(position) = row {
                let mut info = position.library_info().clone();
                for word in &keywords {
                    if !info
                        .keywords
                        .iter()
                        .any(|known| library::fold(known) == library::fold(word))
                    {
                        info.keywords.push(word.clone());
                    }
                }
                position.set_library_info(info);
            }
        }
    }
    if let Some(date) = args
        .opt_text("preis_datum")?
        .filter(|text| !text.trim().is_empty())
    {
        for row in &mut taken.rows {
            if let LvRow::Position(position) = row {
                let mut info = position.library_info().clone();
                info.price_date = date.trim().to_string();
                position.set_library_info(info);
            }
        }
    }

    // The library.
    let source = chosen(server, args, false)?
        .into_iter()
        .next()
        .map(|(source, _)| source)
        .ok_or("Keine Bibliothek gewählt.")?;
    if source.read_only {
        return Err(format!(
            "Die Bibliothek „{}“ ist schreibgeschützt.",
            source.label
        ));
    }
    let existing = source.path.is_file();
    let mut target = if existing {
        Some(server.open_path(&source.path)?)
    } else {
        None
    };
    let mut fresh = new_library(&chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
    let library = match target.as_mut() {
        Some(loaded) => &mut loaded.project,
        None => &mut fresh,
    };
    let title = match (args.opt_text("titel")?, args.opt_text("neuer_titel")?) {
        (Some(oz), _) if !oz.trim().is_empty() => find_by_ordinal(library.lv(), oz)?,
        (_, Some(text)) if !text.trim().is_empty() => {
            let lv = library.lv_mut();
            let end = lv.rows().len();
            let id = lv.insert_numbered(end, LvRow::Group(LvGroup::new(0, "", text.trim())))?;
            lv.index_of(id).ok_or("Titel fehlt nach dem Anlegen.")?
        }
        _ => {
            return Err(
                "Bitte titel (OZ eines Titels der Bibliothek) oder neuer_titel angeben."
                    .to_string(),
            )
        }
    };
    let title_text = library.lv().rows()[title].short_text();
    let range = transfer::insert_under(library, title, &taken)?;
    let answer = format!(
        "{} Zeile(n) ({} Positionen) in die Bibliothek „{}“ unter „{title_text}“ abgelegt (Stand {}).",
        range.len(),
        taken.position_count(),
        source.label,
        today()
    );
    match target {
        Some(loaded) => server.save_with_answer(&loaded, answer),
        None => {
            if let Some(folder) = source.path.parent() {
                std::fs::create_dir_all(folder)
                    .map_err(|error| format!("Ordner {}: {error}", folder.display()))?;
            }
            server.save_new(&source.path, &fresh)?;
            Ok(format!(
                "{answer} Die Bibliothek wurde neu angelegt: {}",
                source.path.display()
            ))
        }
    }
}

pub(crate) fn compare(server: &mut Server, args: &Args) -> Result<String, String> {
    let mut loaded = server.open(args)?;
    let mut libraries = Vec::new();
    let mut problems = Vec::new();
    for source in server.libraries() {
        if !source.path.is_file() {
            continue;
        }
        match server.open_path(&source.path) {
            Ok(library) => libraries.push((source.name(), library.project)),
            Err(error) => problems.push(error),
        }
    }
    let prices = args.opt_flag("preise")?.unwrap_or(false);
    let lvs: Vec<(&str, &proava_domain::Lv)> = libraries
        .iter()
        .map(|(name, project)| (name.as_str(), project.lv()))
        .collect();
    let links = library::links(loaded.project.lv(), &lvs);
    let state = library::library_state(loaded.project.lv(), &lvs, &links, prices);
    let changes: Vec<_> = lv_diff::compare(loaded.project.lv(), &state)
        .changes
        .into_iter()
        .filter(|change| change.kind == ChangeKind::Changed)
        .collect();
    let mut lines = vec![format!(
        "{} Positionen aus einer Bibliothek · {} weichen vom Bibliotheksstand ab{}",
        links.found.len(),
        changes.len(),
        if links.missing.is_empty() {
            String::new()
        } else {
            format!(" · ohne Eintrag: {}", links.missing.join(", "))
        }
    )];
    lines.extend(problems);
    lines.extend(changes.iter().map(|change| change.line()));

    let take: Vec<String> = match args.get("uebernehmen") {
        Some(serde_json::Value::Array(list)) => list
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(text)) => vec![text.clone()],
        _ => Vec::new(),
    };
    if take.is_empty() {
        lines.push("Übernehmen mit uebernehmen: [\"alle\"] oder einer Liste von OZ.".to_string());
        return Ok(lines.join("\n"));
    }
    let chosen: Vec<RowId> = if take
        .iter()
        .any(|text| text.trim().eq_ignore_ascii_case("alle"))
    {
        changes.iter().filter_map(|change| change.new_id).collect()
    } else {
        take.iter()
            .map(|oz| {
                find_by_ordinal(loaded.project.lv(), oz)
                    .map(|index| loaded.project.lv().rows()[index].id())
            })
            .collect::<Result<_, _>>()?
    };
    let projects: Vec<(&str, &Project)> = libraries
        .iter()
        .map(|(name, project)| (name.as_str(), project))
        .collect();
    let (updated, finished) =
        library::refresh_from_library(&mut loaded.project, &projects, &links, &chosen, prices);
    lines.push(format!(
        "{updated} Position(en) auf den Bibliotheksstand gebracht (Menge, OZ, Status bleiben)."
    ));
    if !finished.is_empty() {
        lines.push(format!(
            "Fertig und gesperrt, nicht geändert: {}",
            finished.join(", ")
        ));
    }
    if updated == 0 {
        return Ok(lines.join("\n"));
    }
    server.save_with_answer(&loaded, lines.join("\n"))
}
