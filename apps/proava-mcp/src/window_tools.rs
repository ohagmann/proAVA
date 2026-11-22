//! The window tools: the assistant works in the open proAVA windows over
//! the local bridge (`proava-bridge-protocol`) instead of in the files –
//! changes run through the normal editing of the program (one Strg+Z takes
//! them back, no clash with unsaved changes), and the user sees the rows the
//! assistant works on (KI-Markierung).
//!
//! - `fenster_lesen`: the open windows (links/rechts from their place on the
//!   screen, aktiv = used last) or rows of one window – reads only.
//! - `fenster_zeigen`: select, scroll, open titles, mark – changes nothing.
//! - `fenster_suchen`: the search of „Übernehmen aus Projekt“ in a window.
//! - `fenster_einfuegen`: rows from another window or a file at the cursor of
//!   a window (asks the user: it changes the LV in the window).
//! - `fenster_projekt_oeffnen`: a project in a new window (asks).
//! - `fenster_speichern`: Strg+S in a window (asks), with a backup first.
//!
//! And every other tool on a project that is open in a window works there
//! (`Server::open_path`/`Server::save`): it gets the window's state
//! ([`Command::Stand`]), runs on it as on the file and hands the result
//! back ([`Command::Uebernehmen`]) – one undo step in the window, nothing
//! saved.

use std::path::{Path, PathBuf};
use std::time::Duration;

use proava_application::window_bridge::{pick_rows, without_contained};
use proava_bridge_protocol::{
    names, pick, Command, Insert, Mark, OpenProject, Read, Reply, RowInfo, Rows, Search, Show,
    Take, WindowEntry, WindowInfo,
};
use proava_domain::transfer::{self, TransferOptions};
use proava_domain::RowId;

use crate::args::Args;
use crate::server::Server;

/// The open windows and the way to ask them (the real folder in `main.rs`,
/// tests a recording stand-in – no window, no socket).
pub trait WindowLink {
    /// Entries of the running windows.
    fn windows(&self) -> Vec<WindowEntry>;
    /// Asks one window; waits at most `timeout`.
    fn ask(
        &self,
        window: &WindowEntry,
        command: Command,
        timeout: Duration,
    ) -> Result<Reply, String>;
    /// Where whole projects pass to and from the windows (see
    /// `proava_bridge_protocol::EXCHANGE_FOLDER`); None = no bridge.
    fn exchange_folder(&self) -> Option<PathBuf> {
        None
    }
}

/// The windows of this user: folder `$XDG_RUNTIME_DIR/proava` (Windows
/// `%LOCALAPPDATA%\proava\fenster`), channels from their entries.
pub struct BridgeWindows {
    folder: Option<proava_infrastructure::window_bridge::BridgeFolder>,
    next: std::cell::Cell<u64>,
}

impl BridgeWindows {
    pub fn standard() -> Self {
        Self::in_folder(proava_infrastructure::window_bridge::BridgeFolder::standard())
    }

    pub fn in_folder(folder: Option<proava_infrastructure::window_bridge::BridgeFolder>) -> Self {
        Self {
            folder,
            next: std::cell::Cell::new(1),
        }
    }
}

impl WindowLink for BridgeWindows {
    fn windows(&self) -> Vec<WindowEntry> {
        self.folder
            .as_ref()
            .map(proava_infrastructure::window_bridge::open_windows)
            .unwrap_or_default()
    }

    fn ask(
        &self,
        window: &WindowEntry,
        command: Command,
        timeout: Duration,
    ) -> Result<Reply, String> {
        let id = self.next.get();
        self.next.set(id + 1);
        let request = proava_bridge_protocol::Request::new(id, command);
        proava_infrastructure::window_bridge::ask(&window.kanal, &request, timeout)
            .map(|response| response.antwort)
    }

    fn exchange_folder(&self) -> Option<PathBuf> {
        self.folder.as_ref().map(|folder| folder.exchange_folder())
    }
}

/// Waiting for a window that only reads or shows.
const QUICK: Duration = Duration::from_secs(5);
/// Waiting for an insert (a question of proAVA may wait for the user).
const INSERT: Duration = Duration::from_secs(75);
/// Waiting for a new window after `fenster_projekt_oeffnen`.
const NEW_WINDOW: Duration = Duration::from_secs(20);

const NO_WINDOW: &str = "Kein proAVA-Fenster offen, das KI-Assistenten erlaubt – der Anwender startet proAVA \
(oder schaltet unter Extras ▸ Einstellungen ▸ Darstellung „KI darf geöffnete Fenster bedienen“ ein). \
Ohne Fenster gehen die Datei-Werkzeuge (lv_lesen, positionen_uebernehmen …).";

/// The windows with their current state (asked live; a window that does not
/// answer keeps the state of its entry).
struct Open {
    entries: Vec<WindowEntry>,
    infos: Vec<WindowInfo>,
    stale: Vec<bool>,
}

impl Open {
    fn of(server: &Server) -> Result<Self, String> {
        let link = server.window_link().ok_or(NO_WINDOW)?;
        let entries = link.windows();
        let mut infos = Vec::new();
        let mut stale = Vec::new();
        for entry in &entries {
            match link.ask(entry, Command::FensterLesen, QUICK) {
                Ok(Reply::Fenster(info)) => {
                    infos.push(info);
                    stale.push(false);
                }
                _ => {
                    infos.push(entry.fenster.clone());
                    stale.push(true);
                }
            }
        }
        Ok(Self {
            entries,
            infos,
            stale,
        })
    }

    /// The window meant by `fenster` (Standard: aktiv), only one that allows assistants.
    fn pick(&self, wanted: Option<&str>) -> Result<usize, String> {
        if self.entries.is_empty() {
            return Err(NO_WINDOW.to_string());
        }
        let index = pick(&self.infos, wanted.unwrap_or("aktiv"))?;
        if !self.infos[index].ki_erlaubt {
            return Err(format!(
                "Im Fenster {} sind KI-Assistenten ausgeschaltet (Extras ▸ Einstellungen ▸ Darstellung ▸ \
                 KI-Assistenten) – bitte den Anwender fragen.",
                self.name(index)
            ));
        }
        Ok(index)
    }

    /// „Fenster rechts“, „Fenster PID 4711“ – short, for texts in the windows.
    fn short_name(&self, index: usize) -> String {
        let names = names(&self.infos);
        match names[index].iter().find(|name| **name != "aktiv") {
            Some(side) if self.infos.len() > 1 => format!("Fenster {side}"),
            _ => format!("Fenster PID {}", self.infos[index].pid),
        }
    }

    /// „links (PID 4711, Rathaus 3)“.
    fn name(&self, index: usize) -> String {
        let names = names(&self.infos);
        let info = &self.infos[index];
        let project = info
            .projekt
            .as_ref()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "Startseite".to_string());
        match names[index].iter().find(|name| **name != "aktiv") {
            Some(side) if self.infos.len() > 1 => format!("{side} (PID {}, {project})", info.pid),
            _ => format!("PID {} ({project})", info.pid),
        }
    }
}

fn ask(
    server: &Server,
    open: &Open,
    index: usize,
    command: Command,
    timeout: Duration,
) -> Result<Reply, String> {
    let link = server.window_link().ok_or(NO_WINDOW)?;
    let reply = link.ask(&open.entries[index], command, timeout)?;
    match reply.refusal() {
        Some(text) => Err(format!("Fenster {}: {text}", open.name(index))),
        None => Ok(reply),
    }
}

fn texts(args: &Args, key: &str) -> Result<Vec<String>, String> {
    match args.get(key) {
        None => Ok(Vec::new()),
        Some(serde_json::Value::String(text)) => Ok(vec![text.clone()]),
        Some(serde_json::Value::Array(list)) => list
            .iter()
            .map(|value| match value {
                serde_json::Value::String(text) => Ok(text.clone()),
                serde_json::Value::Number(number) => Ok(number.to_string()),
                _ => Err(format!("{key}: bitte eine Liste von Texten angeben.")),
            })
            .collect(),
        Some(_) => Err(format!("{key}: bitte eine Liste von Texten angeben.")),
    }
}

fn ids(args: &Args, key: &str) -> Result<Vec<u64>, String> {
    let values = match args.get(key) {
        None => return Ok(Vec::new()),
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
            number.filter(|number| *number > 0).ok_or_else(|| {
                format!("{key}: „{value}“ ist keine Zeilen-Id (Zahl wie 12 oder „#12“).")
            })
        })
        .collect()
}

/// `oz`, `ids`, `auswahl`, `weitere`, `lv` as rows of a window.
fn rows(args: &Args, lv_key: &str) -> Result<Rows, String> {
    Ok(Rows {
        lv: args
            .opt_text(lv_key)?
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        auswahl: args.flag("auswahl")?,
        oz: texts(args, "oz")?,
        ids: ids(args, "ids")?,
        weitere: args.opt_count("weitere")?.unwrap_or(0) as usize,
    })
}

fn row_lines(rows: &[RowInfo], long_text: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(format!("  {}", row.line()));
        if !row.titelpfad.is_empty() && row.art != "Titel" {
            lines.push(format!("      in {}", row.titelpfad));
        }
        if !row.fundstelle.is_empty() {
            lines.push(format!("      {}", row.fundstelle));
        }
        if long_text {
            if let Some(text) = row
                .langtext
                .as_deref()
                .filter(|text| !text.trim().is_empty())
            {
                for line in text.lines() {
                    lines.push(format!("      | {line}"));
                }
            }
        }
    }
    lines
}

// ----- fenster_lesen -----

pub(crate) fn read(server: &mut Server, args: &Args) -> Result<String, String> {
    let open = Open::of(server)?;
    let wanted = args
        .opt_text("fenster")?
        .filter(|text| !text.trim().is_empty());
    let reads_rows = args.get("zeilen").is_some()
        || args.get("ab").is_some()
        || args.get("oz").is_some()
        || args.get("ids").is_some()
        || args.flag("auswahl")?;
    if wanted.is_none() && !reads_rows {
        return Ok(listing(&open));
    }
    let index = open.pick(wanted)?;
    if !reads_rows {
        return Ok(details(&open, index));
    }
    let mut rows = rows(args, "lv")?;
    if let Some(text) = args.opt_text("zeilen")? {
        if text.trim().eq_ignore_ascii_case("auswahl") {
            rows.auswahl = true;
        } else {
            rows.oz.extend(
                text.split(',')
                    .map(|oz| oz.trim().to_string())
                    .filter(|oz| !oz.is_empty()),
            );
        }
    }
    let read = Read {
        zeilen: rows,
        ab: args
            .opt_text("ab")?
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        anzahl: args.opt_count("anzahl")?.map(|count| count as usize),
        langtext: args.flag("langtext")?,
    };
    let long_text = read.langtext;
    let Reply::Zeilen(reply) = ask(server, &open, index, Command::Lesen(read), QUICK)? else {
        return Err("Unerwartete Antwort des Fensters.".to_string());
    };
    let mut lines = vec![format!(
        "Fenster {} · {} · {}:",
        open.name(index),
        reply.lv.as_ref().map(|lv| lv.label()).unwrap_or_default(),
        count(reply.gesamt, "Zeile", "Zeilen")
    )];
    lines.extend(row_lines(&reply.zeilen, long_text));
    if reply.gesamt > reply.zeilen.len() {
        lines.push(format!(
            "  … {} weitere (anzahl erhöhen)",
            reply.gesamt - reply.zeilen.len()
        ));
    }
    Ok(lines.join("\n"))
}

fn listing(open: &Open) -> String {
    if open.entries.is_empty() {
        return "Kein proAVA-Fenster offen (oder keines meldet sich an der Brücke). Der Anwender startet proAVA; \
                ohne Fenster gehen die Datei-Werkzeuge."
            .to_string();
    }
    let names = names(&open.infos);
    let mut lines = vec![format!(
        "{} offen – links/rechts nach der Lage auf dem Bildschirm, aktiv = zuletzt benutzt. Fenster mit \
         fenster: „links“, „rechts“, „aktiv“, Projektname oder PID ansprechen.",
        count(open.entries.len(), "proAVA-Fenster", "proAVA-Fenster")
    )];
    for (index, info) in open.infos.iter().enumerate() {
        let label = if names[index].is_empty() {
            String::new()
        } else {
            format!("[{}] ", names[index].join(", "))
        };
        lines.push(format!(
            "{label}PID {} · {}",
            info.pid,
            window_summary(info)
        ));
        if open.stale[index] {
            lines.push(
                "    antwortet gerade nicht (beschäftigt?) – Stand der letzten Meldung".to_string(),
            );
        }
        if !info.ki_erlaubt {
            lines.push("    KI-Bedienung in diesem Fenster ausgeschaltet".to_string());
        }
        if let Some(project) = &info.projekt {
            lines.push(format!("    Datei: {}", project.datei));
            for row in project.auswahl.iter().take(5) {
                lines.push(format!("    Auswahl: {}", row.line()));
            }
            if project.auswahl.len() > 5 {
                lines.push(format!(
                    "    … {} weitere gewählt",
                    project.auswahl.len() - 5
                ));
            }
        }
        if let Some(dialog) = &info.dialog {
            lines.push(format!(
                "    Dialog offen: {dialog} (wartet auf den Anwender)"
            ));
        }
        if let Some(activity) = &info.ki_taetigkeit {
            lines.push(format!("    KI: {activity}"));
        }
    }
    lines.join("\n")
}

fn window_summary(info: &WindowInfo) -> String {
    let mut parts = Vec::new();
    match &info.projekt {
        Some(project) => {
            parts.push(project.name.clone());
            parts.push(project.lv.label());
            if project.lvs.len() > 1 {
                parts.push(format!("{} LVs", project.lvs.len()));
            }
            if project.reiter != "LV" {
                parts.push(format!("Reiter {}", project.reiter));
            }
            if project.schreibgeschuetzt {
                parts.push("schreibgeschützt".to_string());
            } else if project.ungespeichert {
                parts.push("ungespeichert".to_string());
            } else {
                parts.push("gespeichert".to_string());
            }
            if project.versandt {
                parts.push("versandt".to_string());
            }
        }
        None => parts.push("Startseite (kein Projekt offen)".to_string()),
    }
    if info.minimiert {
        parts.push("minimiert".to_string());
    }
    if let Some(place) = info.lage {
        parts.push(format!(
            "Lage {}/{} {}×{}",
            place.x.round(),
            place.y.round(),
            place.breite.round(),
            place.hoehe.round()
        ));
    }
    parts.join(" · ")
}

fn details(open: &Open, index: usize) -> String {
    let info = &open.infos[index];
    let mut lines = vec![format!(
        "Fenster {} · {}",
        open.name(index),
        window_summary(info)
    )];
    if let Some(project) = &info.projekt {
        lines.push(format!("Datei: {}", project.datei));
        if project.lvs.len() > 1 {
            lines.push("LVs:".to_string());
            for lv in &project.lvs {
                let mark = if lv.stelle == project.lv.stelle {
                    " (angezeigt)"
                } else {
                    ""
                };
                lines.push(format!(
                    "  {} = {} · {}{mark}",
                    lv.stelle,
                    lv.label(),
                    count(lv.positionen, "Position", "Positionen")
                ));
            }
        }
        if project.auswahl.is_empty() {
            lines.push("Auswahl: keine".to_string());
        } else {
            lines.push(format!(
                "Auswahl ({}; die erste ist die Schreibmarke – Einfügen kommt dahinter):",
                count(project.auswahl.len(), "Zeile", "Zeilen")
            ));
            lines.extend(row_lines(&project.auswahl, false));
        }
    }
    if let Some(dialog) = &info.dialog {
        lines.push(format!("Dialog offen: {dialog} – wartet auf den Anwender."));
    }
    if let Some(activity) = &info.ki_taetigkeit {
        lines.push(format!("KI: {activity}"));
    }
    lines.join("\n")
}

// ----- fenster_zeigen -----

pub(crate) fn show(server: &mut Server, args: &Args) -> Result<String, String> {
    let open = Open::of(server)?;
    let index = open.pick(args.opt_text("fenster")?)?;
    let rows = rows(args, "lv")?;
    let text = args
        .opt_text("markierung")?
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty());
    if args.flag("aufheben")? {
        ask(
            server,
            &open,
            index,
            Command::Markieren(Mark {
                aufheben: true,
                ..Mark::default()
            }),
            QUICK,
        )?;
        return Ok(format!(
            "Markierung im Fenster {} aufgehoben.",
            open.name(index)
        ));
    }
    if rows.is_empty() {
        return Err(
            "Bitte oz, ids oder auswahl: true angeben (welche Zeilen gezeigt werden).".to_string(),
        );
    }
    let command = if args.flag("nur_markieren")? {
        Command::Markieren(Mark {
            zeilen: rows,
            text: text.unwrap_or_default(),
            sekunden: args
                .opt_count("sekunden")?
                .map(|seconds| seconds.min(u64::from(u32::MAX)) as u32),
            aufheben: false,
        })
    } else {
        Command::Zeigen(Show {
            zeilen: rows,
            positionsfenster: args.flag("positionsfenster")?,
            markierung: text,
        })
    };
    let Reply::Zeilen(reply) = ask(server, &open, index, command, QUICK)? else {
        return Err("Unerwartete Antwort des Fensters.".to_string());
    };
    let mut lines = vec![format!(
        "Fenster {} · {}: {} {}",
        open.name(index),
        reply.lv.as_ref().map(|lv| lv.label()).unwrap_or_default(),
        count(reply.zeilen.len(), "Zeile", "Zeilen"),
        reply.text
    )];
    lines.extend(row_lines(&reply.zeilen, false));
    lines.push(
        "Der Anwender sieht sie farbig umrandet; nach seinem Ja mit fenster_einfuegen (quelle_fenster = dieses \
         Fenster, auswahl: true) übernehmen."
            .to_string(),
    );
    Ok(lines.join("\n"))
}

// ----- fenster_suchen -----

pub(crate) fn search(server: &mut Server, args: &Args) -> Result<String, String> {
    let open = Open::of(server)?;
    let index = open.pick(args.opt_text("fenster")?)?;
    let query = Search {
        suche: args
            .opt_text("suche")?
            .unwrap_or_default()
            .trim()
            .to_string(),
        kg: args.opt_text("kg")?.unwrap_or_default().trim().to_string(),
        einheit: args
            .opt_text("einheit")?
            .unwrap_or_default()
            .trim()
            .to_string(),
        lv: args
            .opt_text("lv")?
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        max_treffer: args.opt_count("max_treffer")?.map(|count| count as usize),
    };
    let Reply::Zeilen(reply) = ask(server, &open, index, Command::Suchen(query), QUICK)? else {
        return Err("Unerwartete Antwort des Fensters.".to_string());
    };
    let mut lines = vec![format!(
        "Fenster {} · {}: {}",
        open.name(index),
        reply.lv.as_ref().map(|lv| lv.label()).unwrap_or_default(),
        reply.text
    )];
    lines.extend(row_lines(&reply.zeilen, false));
    if reply.gesamt > reply.zeilen.len() {
        lines.push(format!(
            "  … {} weitere (max_treffer erhöhen)",
            reply.gesamt - reply.zeilen.len()
        ));
    }
    if reply.gesamt > 0 {
        lines.push(
            "Dem Anwender die Treffer nennen; die gemeinte mit fenster_zeigen (oz/ids) wählen, dann fenster_einfuegen."
                .to_string(),
        );
    }
    Ok(lines.join("\n"))
}

// ----- fenster_einfuegen -----

pub(crate) fn insert(server: &mut Server, args: &Args) -> Result<String, String> {
    let open = Open::of(server)?;
    let target = open.pick(args.opt_text("fenster")?)?;
    let options = TransferOptions {
        quantities: args.opt_flag("mengen")?.unwrap_or(true),
        prices: args.opt_flag("preise")?.unwrap_or(false),
    };
    let source_window = args
        .opt_text("quelle_fenster")?
        .filter(|text| !text.trim().is_empty());
    let source_file = args
        .opt_text("quelle")?
        .filter(|text| !text.trim().is_empty());
    let (data, origin, taken) = match (source_window, source_file) {
        (Some(_), Some(_)) => {
            return Err(
                "Bitte entweder quelle_fenster oder quelle (Datei) angeben, nicht beides."
                    .to_string(),
            )
        }
        (None, None) => return Err(
            "Woher? quelle_fenster („rechts“, Projektname, PID) oder quelle (Projekt-, GAEB- oder \
                 Bibliotheksdatei) angeben."
                .to_string(),
        ),
        (Some(wanted), None) => {
            let source = open.pick(Some(wanted))?;
            if source == target {
                return Err(
                    "Quelle und Ziel sind dasselbe Fenster – zum Verschieben in einem LV ist das kein Weg; \
                     bitte ein anderes Fenster als Ziel nennen."
                        .to_string(),
                );
            }
            let mut rows = rows(args, "quelle_lv")?;
            if rows.is_empty() {
                rows.auswahl = true;
            }
            let target_project = open.infos[target]
                .projekt
                .as_ref()
                .map(|project| project.name.clone())
                .unwrap_or_default();
            let take = Take {
                zeilen: rows,
                mengen: options.quantities,
                preise: options.prices,
                markierung: Some(format!(
                    "kopiert nach {target_project} ({})",
                    open.short_name(target)
                )),
            };
            let Reply::Kopie(copy) = ask(server, &open, source, Command::Kopieren(take), QUICK)?
            else {
                return Err("Unerwartete Antwort des Quellfensters.".to_string());
            };
            (
                copy.daten,
                format!("{} ({})", copy.projekt, open.short_name(source)),
                copy.zeilen,
            )
        }
        (None, Some(path)) => from_file(server, args, Path::new(path.trim()), options)?,
    };
    let command = Command::Einfuegen(Insert {
        daten: data,
        nach: place(args)?,
        lv: args
            .opt_text("lv")?
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        herkunft: origin.clone(),
    });
    let Reply::Erledigt(done) = ask(server, &open, target, command, INSERT)? else {
        return Err("Unerwartete Antwort des Zielfensters.".to_string());
    };
    let mut lines = vec![format!("Fenster {}: {}", open.name(target), done.text)];
    lines.push(format!(
        "Aus {origin} ({}; Mengen {}, Preise {}):",
        count(taken.len(), "Zeile", "Zeilen"),
        if options.quantities {
            "übernommen"
        } else {
            "geleert"
        },
        if options.prices {
            "übernommen"
        } else {
            "geleert"
        }
    ));
    lines.push("Neu im Ziel:".to_string());
    lines.extend(row_lines(&done.zeilen, false));
    if done.wartet {
        lines.push(
            "proAVA wartet mit einer Rückfrage auf den Anwender – erst nach seiner Antwort weitermachen."
                .to_string(),
        );
    }
    Ok(lines.join("\n"))
}

/// The place in the target: after `nach_oz`/`nach_id`, else the cursor.
fn place(args: &Args) -> Result<Option<String>, String> {
    if let Some(id) = args.opt_count("nach_id")? {
        return Ok(Some(format!("#{id}")));
    }
    Ok(args
        .opt_text("nach_oz")?
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty()))
}

/// Rows of a project, library or GAEB file as proAVA clipboard text.
fn from_file(
    server: &Server,
    args: &Args,
    path: &Path,
    options: TransferOptions,
) -> Result<(String, String, Vec<RowInfo>), String> {
    let source = crate::transfer_tools::source_project(server, path, args.get("quelle_lv"))?;
    let lv = source.lv();
    let rows = rows(args, "quelle_lv")?;
    if rows.auswahl {
        return Err(
            "auswahl gilt nur für quelle_fenster – bei einer Datei oz oder ids angeben."
                .to_string(),
        );
    }
    let picked = pick_rows(lv, &Rows { lv: None, ..rows }, &[])?;
    let roots = without_contained(lv, &picked);
    let ids: Vec<RowId> = roots.iter().map(|&index| lv.rows()[index].id()).collect();
    let taken = transfer::collect(&source, lv, &ids, options);
    let clip = proava_application::Clip::Rows(taken);
    let (header, _) =
        proava_application::clipboard::encode(server.repository(), &clip, lv.oz_mask())?;
    let rows = picked
        .iter()
        .map(|&index| proava_application::window_bridge::row_info(lv, index, false))
        .collect();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok((header.line(), name, rows))
}

// ----- fenster_projekt_oeffnen -----

pub(crate) fn open_project(server: &mut Server, args: &Args) -> Result<String, String> {
    let path = PathBuf::from(args.text("projekt")?.trim());
    let path = server.allowed(&path)?;
    let real = crate::real_path(&path).unwrap_or(path.clone());
    let open = Open::of(server)?;
    if open.entries.is_empty() {
        return Err(
            "Kein proAVA-Fenster offen – ein neues Fenster öffnet nur ein laufendes proAVA. Der Anwender startet \
             proAVA einmal (Startmenü)."
                .to_string(),
        );
    }
    if let Some(index) = open.infos.iter().position(|info| {
        info.projekt
            .as_ref()
            .is_some_and(|project| same_file(Path::new(&project.datei), &real))
    }) {
        return Ok(format!(
            "„{}“ ist schon offen im Fenster {} – fenster_suchen/fenster_zeigen mit fenster = „{}“.",
            file_stem(&real),
            open.name(index),
            open.infos[index].pid
        ));
    }
    // A window on the start page opens it itself, else the active one starts a new window.
    let opener = match args
        .opt_text("fenster")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(wanted) => open.pick(Some(wanted))?,
        None => open
            .infos
            .iter()
            .position(|info| info.projekt.is_none() && info.ki_erlaubt)
            .map_or_else(|| open.pick(None), Ok)?,
    };
    let known: Vec<u32> = open.entries.iter().map(|entry| entry.pid).collect();
    let command = Command::ProjektOeffnen(OpenProject {
        pfad: real.to_string_lossy().into_owned(),
        schreibgeschuetzt: args.flag("schreibgeschuetzt")?,
    });
    let Reply::Erledigt(done) = ask(server, &open, opener, command, QUICK)? else {
        return Err("Unerwartete Antwort des Fensters.".to_string());
    };
    let mut lines = vec![done.text];
    // Wait for the window with the project (new window, or the start page).
    let link = server.window_link().ok_or(NO_WINDOW)?;
    let start = std::time::Instant::now();
    let mut found = None;
    while start.elapsed() < NEW_WINDOW {
        let entries = link.windows();
        found = entries.into_iter().find(|entry| {
            entry.fenster.projekt.as_ref().is_some_and(|project| {
                same_file(Path::new(&project.datei), &real)
                    && (!known.contains(&entry.pid) || entry.pid == open.infos[opener].pid)
            })
        });
        if found.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    match found {
        Some(entry) => lines.push(format!(
            "Offen im Fenster PID {} ({}). Weiter mit fenster_suchen (fenster = „{}“) oder fenster_lesen.",
            entry.pid,
            entry.fenster.label(),
            entry.pid
        )),
        None => lines.push(
            "Das Fenster hat sich noch nicht gemeldet (Rückfrage im Fenster, oder es braucht länger) – \
             gleich mit fenster_lesen nachsehen."
                .to_string(),
        ),
    }
    Ok(lines.join("\n"))
}

fn same_file(a: &Path, b: &Path) -> bool {
    let a = crate::real_path(a).unwrap_or_else(|_| a.to_path_buf());
    let b = crate::real_path(b).unwrap_or_else(|_| b.to_path_buf());
    a == b
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ----- Alle Werkzeuge im offenen Fenster -----

/// Waiting for the state of a window (it writes the whole project).
const STATE: Duration = Duration::from_secs(60);
/// Waiting for a window to take over a change (a question may wait for the user).
const ADOPT: Duration = INSERT;

/// A project open in a window, as a tool works on it.
pub(crate) struct WindowState {
    pub entry: WindowEntry,
    /// Number of the state; the window takes a change over only on it.
    pub stand: u64,
}

/// What [`window_state`] found.
pub(crate) enum FromWindow {
    State(Box<proava_domain::Project>, Box<WindowState>),
    /// Assistants are switched off in the window: tools may read the file
    /// but not change it (the text says why).
    Off(String),
}

/// The editable window that has `path` open (read-only windows do not count).
pub(crate) fn window_with(server: &Server, path: &Path) -> Option<WindowEntry> {
    let link = server.window_link()?;
    link.windows().into_iter().find(|entry| {
        entry.fenster.projekt.as_ref().is_some_and(|project| {
            same_file(Path::new(&project.datei), path) && !project.schreibgeschuetzt
        })
    })
}

/// A file the window wrote or will read: only inside the exchange folder.
fn exchange_file(server: &Server, file: &Path) -> Result<PathBuf, String> {
    let folder = server
        .window_link()
        .and_then(|link| link.exchange_folder())
        .ok_or(NO_WINDOW)?;
    let inside = file
        .parent()
        .is_some_and(|parent| same_file(parent, &folder));
    if inside {
        Ok(file.to_path_buf())
    } else {
        Err(format!(
            "Das Fenster nannte eine Datei außerhalb des Austauschordners: {}",
            file.display()
        ))
    }
}

/// The project `path` as the window has it now (also unsaved).
pub(crate) fn window_state(
    server: &Server,
    entry: &WindowEntry,
    path: &Path,
) -> Result<FromWindow, String> {
    let link = server.window_link().ok_or(NO_WINDOW)?;
    match link.ask(entry, Command::Stand, STATE)? {
        Reply::Stand(state) => {
            let file = exchange_file(server, Path::new(&state.datei))?;
            let project = server.repository().load_project(&file);
            let _ = std::fs::remove_file(&file);
            Ok(FromWindow::State(
                Box::new(project?),
                Box::new(WindowState {
                    entry: entry.clone(),
                    stand: state.stand,
                }),
            ))
        }
        Reply::Aus { .. } => Ok(FromWindow::Off(format!(
            "Nichts geändert: {} ist in proAVA offen (Fenster PID {}), und dort dürfen KI-Assistenten \
             nicht arbeiten. Der Anwender schaltet „KI darf geöffnete Fenster bedienen“ ein (Extras ▸ \
             Einstellungen ▸ Darstellung) oder speichert und schließt das Projekt dort.",
            path.display(),
            entry.pid
        ))),
        reply => Err(window_refusal(entry, &reply)),
    }
}

fn window_refusal(entry: &WindowEntry, reply: &Reply) -> String {
    match reply.refusal() {
        Some(text) if text.contains("unknown variant") => format!(
            "Das proAVA im Fenster PID {} ist älter als proava-mcp – der Anwender startet proAVA neu.",
            entry.pid
        ),
        Some(text) => format!("Fenster PID {}: {text}", entry.pid),
        None => format!("Fenster PID {}: unerwartete Antwort.", entry.pid),
    }
}

/// Hands the project a tool changed back to its window: one undo step
/// there, nothing saved. Returns the window's text for the answer.
pub(crate) fn adopt(
    server: &Server,
    project: &proava_domain::Project,
    window: &WindowState,
    tool: &str,
) -> Result<String, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let link = server.window_link().ok_or(NO_WINDOW)?;
    let folder = link.exchange_folder().ok_or(NO_WINDOW)?;
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("Austauschordner {}: {error}", folder.display()))?;
    let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let file = folder.join(format!("mcp-{}-{number}.proava", std::process::id()));
    let written = server.repository().save_project(&file, project);
    let reply = written.and_then(|()| {
        link.ask(
            &window.entry,
            Command::Uebernehmen(proava_bridge_protocol::Adopt {
                datei: file.to_string_lossy().into_owned(),
                stand: window.stand,
                werkzeug: tool.to_string(),
            }),
            ADOPT,
        )
    });
    let _ = std::fs::remove_file(&file);
    match reply? {
        Reply::Erledigt(done) => Ok(format!("Fenster PID {}: {}", window.entry.pid, done.text)),
        reply => Err(format!(
            "Nichts geändert – {}",
            window_refusal(&window.entry, &reply)
        )),
    }
}

// ----- fenster_speichern -----

/// Strg+S in the window that has `projekt` open (or `fenster`); before the
/// first save of a file in this server run a backup next to it.
pub(crate) fn save(server: &mut Server, args: &Args) -> Result<String, String> {
    let entry = match args
        .opt_text("projekt")?
        .filter(|text| !text.trim().is_empty())
    {
        Some(path) => {
            let path = server.allowed(Path::new(path.trim()))?;
            window_with(server, &path).ok_or_else(|| {
                format!(
                    "{} ist in keinem proAVA-Fenster zum Bearbeiten offen.",
                    path.display()
                )
            })?
        }
        None => {
            let open = Open::of(server)?;
            let index = open.pick(args.opt_text("fenster")?)?;
            open.entries[index].clone()
        }
    };
    let file = entry
        .fenster
        .projekt
        .as_ref()
        .map(|project| PathBuf::from(&project.datei))
        .ok_or("In diesem Fenster ist kein Projekt offen.")?;
    let backup = server.backup_once(&file)?;
    let link = server.window_link().ok_or(NO_WINDOW)?;
    let reply = link.ask(&entry, Command::Speichern, ADOPT)?;
    let Reply::Erledigt(done) = &reply else {
        return Err(window_refusal(&entry, &reply));
    };
    Ok(match backup {
        Some(backup) => format!("{}\nSicherung vorher: {}", done.text, backup.display()),
        None => done.text.clone(),
    })
}

/// „1 Zeile“, „3 Zeilen“.
fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}
