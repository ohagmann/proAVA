//! `lv_suchen`: full-text search in one project, one GAEB file or all
//! projects below a folder – the finding half of „Hole die Positionen für
//! die Raumbediengeräte aus dem LV von … und füge sie hier ein“ (the taking
//! half is `positionen_uebernehmen`). Search and file walk live in the
//! domain and application (`lv_search`); here only arguments and answer.

use std::path::Path;
use std::time::{Duration, Instant};

use proava_application::lv_search::{
    self, LvHits, SearchOutcome, SourceHits, SourceKind, MAX_FILES,
};
use proava_domain::lv_search::{HitField, HitKind, LvHit, LvQuery};

use crate::args::Args;
use crate::format::{price, quantity};
use crate::server::Server;

/// Standard and highest time limit in seconds.
const DEFAULT_SECONDS: u64 = 20;
const MAX_SECONDS: u64 = 120;
/// Standard number of hits listed.
const DEFAULT_HITS: usize = 50;
/// Unreadable files named in the answer (the rest only counted).
const SKIPPED_LISTED: usize = 10;

fn now_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub(crate) fn search(server: &mut Server, args: &Args) -> Result<String, String> {
    let query = LvQuery {
        text: args
            .opt_text("suche")?
            .unwrap_or_default()
            .trim()
            .to_string(),
        cost_group: args.opt_text("kg")?.unwrap_or_default().trim().to_string(),
        unit: args
            .opt_text("einheit")?
            .unwrap_or_default()
            .trim()
            .to_string(),
    };
    if query.is_empty() {
        return Err("Bitte suche (Wörter), kg oder einheit angeben.".to_string());
    }
    let targets: Vec<&str> = ["projekt", "ordner", "datei"]
        .into_iter()
        .filter(|key| {
            args.opt_text(key)
                .ok()
                .flatten()
                .is_some_and(|text| !text.trim().is_empty())
        })
        .collect();
    if targets.len() != 1 {
        return Err(
            "Bitte genau eines angeben: projekt (eine .proava), ordner (alle Projekte darunter) \
             oder datei (GAEB-LV X81–X86/D81–D86 oder CSV)."
                .to_string(),
        );
    }
    let limit = args
        .opt_count("max_treffer")?
        .map(|count| count.max(1) as usize)
        .unwrap_or(DEFAULT_HITS);
    let seconds = args
        .opt_count("zeitlimit_s")?
        .unwrap_or(DEFAULT_SECONDS)
        .clamp(1, MAX_SECONDS);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let now = now_iso();

    let (outcome, scope, more) = match targets[0] {
        "projekt" => {
            let path = server.allowed(Path::new(args.text("projekt")?.trim()))?;
            if lv_search::source_kind(&path) != Some(SourceKind::Project) {
                return Err(format!(
                    "Keine proAVA-Projektdatei (.proava): {} – GAEB-Dateien mit datei.",
                    path.display()
                ));
            }
            if !path.is_file() {
                return Err(format!("Projektdatei nicht gefunden: {}", path.display()));
            }
            let project = server.repository().load_project_texts(&path)?;
            let lv = match args.get("lv").filter(|value| !value.is_null()) {
                Some(value) => Some(crate::lv_tools::select_lv(&project, Some(value))?),
                None => None,
            };
            let lvs = lv_search::search_project(&project, lv, &query);
            let mut outcome = SearchOutcome {
                searched: 1,
                ..SearchOutcome::default()
            };
            if !lvs.is_empty() {
                outcome.sources.push(SourceHits {
                    path: path.clone(),
                    kind: SourceKind::Project,
                    name: project.name().to_string(),
                    lvs,
                });
            }
            (outcome, format!("im Projekt {}", path.display()), false)
        }
        "datei" => {
            let path = server.allowed(Path::new(args.text("datei")?.trim()))?;
            if !path.is_file() {
                return Err(format!("Datei nicht gefunden: {}", path.display()));
            }
            if lv_search::source_kind(&path) != Some(SourceKind::Gaeb)
                && !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
            {
                return Err(format!(
                    "datei: bitte ein GAEB-LV (X81–X86, D81–D86, auch in .zip) oder eine CSV – \
                     proAVA-Projekte mit projekt: {}",
                    path.display()
                ));
            }
            let name = proava_domain::ProjectName::parse(
                &path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().to_string())
                    .unwrap_or_default(),
            )
            .or_else(|_| proava_domain::ProjectName::parse("GAEB"))
            .map_err(|error| error.to_string())?;
            let project = server.exchange().read_lv(&path, &name, &now)?.project;
            let lvs = lv_search::search_project(&project, None, &query);
            let mut outcome = SearchOutcome {
                searched: 1,
                ..SearchOutcome::default()
            };
            if !lvs.is_empty() {
                outcome.sources.push(SourceHits {
                    path: path.clone(),
                    kind: SourceKind::Gaeb,
                    name: project.name().to_string(),
                    lvs,
                });
            }
            (outcome, format!("in {}", path.display()), false)
        }
        _ => {
            let folder = server.allowed(Path::new(args.text("ordner")?.trim()))?;
            if !folder.is_dir() {
                return Err(format!("Ordner nicht gefunden: {}", folder.display()));
            }
            let with_old = args.flag("mit_alten_versionen")?;
            let scan = {
                let server = &*server;
                lv_search::project_files(server.repository(), &folder, with_old, &|path| {
                    server.allowed(path).is_ok()
                })
            };
            let outcome = lv_search::search_files(
                server.repository(),
                server.exchange(),
                &scan.files,
                &query,
                &now,
                &mut || Instant::now() >= deadline,
            );
            (
                outcome,
                format!(
                    "in {} unter {}",
                    count(scan.files.len(), "Projekt", "Projekten"),
                    folder.display()
                ),
                scan.more,
            )
        }
    };

    let mut lines = vec![summary(&query, &scope, &outcome, started.elapsed())];
    if outcome.hit_count() > 0 {
        lines.push(
            "Übernehmen: positionen_uebernehmen mit quelle = Datei wie unten, quelle_lv = LV wie \
             unten (bei mehreren LVs), oz = [OZ …] (ein Titel bringt seinen Inhalt mit) oder \
             ids = [Id …] (auch Hinweise ohne OZ); Ort im Ziel mit nach_oz oder unter_titel."
                .to_string(),
        );
    }
    let mut listed = 0;
    for source in &outcome.sources {
        lines.push(String::new());
        lines.extend(source_lines(source, limit, &mut listed));
    }
    if listed < outcome.hit_count() {
        lines.push(format!(
            "… {} weitere Treffer nicht aufgeführt (max_treffer erhöhen oder suche/kg/einheit enger fassen).",
            outcome.hit_count() - listed
        ));
    }
    if !outcome.skipped.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Übersprungen, nicht lesbar: {}",
            count(outcome.skipped.len(), "Datei", "Dateien")
        ));
        for (path, reason) in outcome.skipped.iter().take(SKIPPED_LISTED) {
            let reason = reason
                .strip_prefix(&format!("{}: ", path.display()))
                .unwrap_or(reason);
            lines.push(format!("  {} – {reason}", path.display()));
        }
        if outcome.skipped.len() > SKIPPED_LISTED {
            lines.push(format!(
                "  … {} weitere",
                outcome.skipped.len() - SKIPPED_LISTED
            ));
        }
    }
    if outcome.stopped() {
        lines.push(format!(
            "Zeitlimit von {seconds} s erreicht: {} nicht durchsucht – ordner enger fassen oder zeitlimit_s erhöhen (höchstens {MAX_SECONDS}).",
            count(outcome.not_reached, "Projekt", "Projekte")
        ));
    }
    if more {
        lines.push(format!(
            "Mehr als {MAX_FILES} Projekte unter dem Ordner – nur die ersten {MAX_FILES} durchsucht; ordner enger fassen."
        ));
    }
    Ok(lines.join("\n"))
}

fn count(number: usize, one: &str, many: &str) -> String {
    format!("{number} {}", if number == 1 { one } else { many })
}

fn summary(query: &LvQuery, scope: &str, outcome: &SearchOutcome, took: Duration) -> String {
    let mut what = Vec::new();
    if !query.text.trim().is_empty() {
        what.push(format!("„{}“", query.text.trim()));
    }
    if !query.cost_group.trim().is_empty() {
        what.push(format!("KG {}", query.cost_group.trim()));
    }
    if !query.unit.trim().is_empty() {
        what.push(format!("Einheit {}", query.unit.trim()));
    }
    let hits = outcome.hit_count();
    let found = if hits == 0 {
        "keine Treffer".to_string()
    } else {
        format!(
            "{} in {}",
            count(hits, "Treffer", "Treffer"),
            count(outcome.sources.len(), "Datei", "Dateien")
        )
    };
    let seconds = format!("{:.1}", took.as_secs_f64()).replace('.', ",");
    format!("Suche {} {scope}: {found} ({seconds} s).", what.join(", "))
}

fn source_lines(source: &SourceHits, limit: usize, listed: &mut usize) -> Vec<String> {
    let mut lines = vec![match source.kind {
        SourceKind::Project => format!(
            "=== Projekt „{}“ – {} ({})",
            source.name,
            source.path.display(),
            count(source.count(), "Treffer", "Treffer")
        ),
        SourceKind::Gaeb => format!(
            "=== Datei {} ({})",
            source.path.display(),
            count(source.count(), "Treffer", "Treffer")
        ),
    }];
    for lv in &source.lvs {
        lines.push(lv_line(lv));
        for hit in &lv.hits {
            if *listed >= limit {
                break;
            }
            *listed += 1;
            lines.extend(hit_lines(hit));
        }
    }
    lines
}

fn lv_line(lv: &LvHits) -> String {
    let choice = if lv.number.trim().is_empty() {
        (lv.lv_index + 1).to_string()
    } else {
        lv.number.trim().to_string()
    };
    format!(
        "  LV {} ({}) · quelle_lv: \"{choice}\"",
        lv.label(),
        count(lv.hits.len(), "Treffer", "Treffer")
    )
}

fn hit_lines(hit: &LvHit) -> Vec<String> {
    let group = hit
        .cost_group
        .as_deref()
        .map(|code| format!(" | KG {code}"))
        .unwrap_or_default();
    let first = match hit.kind {
        HitKind::Position => format!(
            "    {} | Pos | {} | {} {} | EP {}{group} | #{}",
            hit.ordinal,
            hit.short_text,
            hit.quantity.map(quantity).unwrap_or_default(),
            hit.unit,
            price(hit.unit_price),
            hit.id.0
        ),
        HitKind::Title => format!(
            "    {} | Titel | {} ({}){group} | #{}",
            hit.ordinal,
            hit.short_text,
            count(hit.positions, "Position", "Positionen"),
            hit.id.0
        ),
        HitKind::Note => format!("    – | Hinweis | {} | #{}", hit.short_text, hit.id.0),
        HitKind::Performance => format!(
            "    {} | Ausführungsbeschreibung | {} | #{}",
            hit.ordinal, hit.short_text, hit.id.0
        ),
    };
    let mut lines = vec![first];
    if !hit.path.is_empty() {
        lines.push(format!("      in: {}", hit.path_text()));
    }
    if !hit.excerpt.is_empty() {
        // A Kurztext hit shows the beginning of the long text.
        let label = match hit.field {
            HitField::ShortText => HitField::LongText.label(),
            field => field.label(),
        };
        lines.push(format!("      {label}: „{}“", hit.excerpt));
    }
    lines
}
