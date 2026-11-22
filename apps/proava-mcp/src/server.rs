//! JSON-RPC 2.0 over stdio, tool dispatch and safe access to project files.

use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use proava_application::{ExchangeFiles, ProjectRepository};
use proava_domain::Project;
use serde_json::{json, Map, Value};

use crate::args::Args;
use crate::{catalog, exchange_tools, read_tools, write_tools};

pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const KNOWN_PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
pub(crate) const PROJECT_EXTENSION: &str = "proava";

/// Everything the tools share; MCP clients (e.g. opencode) send it to the
/// model once per session (the tool descriptions stay short, see `catalog.rs`).
const INSTRUCTIONS: &str = "proAVA: Leistungsverzeichnisse (LV) in .proava-Projektdateien lesen und schreiben.

Grundregeln
- Erst lesen (projekt_lesen, lv_lesen), dann ändern. Pfade immer absolut; GAEB-Dateien auch in einer .zip.
- lv: LV-Nummer („2“, „481“) oder Position aus lvs_lesen (1 = erstes; die Nummer geht vor); ohne Angabe das aktive LV (in fenster_* das angezeigte). In kosten_* der Datenpunkt-Abschnitt dieses Kosten-LV (abschnitt = OZ seines Titels); kostenermittlung = Nummer oder Bezeichnung, ohne Angabe die neueste.
- Zeilen: oz wie in lv_lesen („01.02.0010“, führende Nullen dürfen fehlen) oder id (#12) für Hinweise und doppelte OZ. Andere Ids aus den *_lesen-Werkzeugen; nachtrag „N03“ oder 3.
- Ort beim Einfügen: nach_oz/nach_id = hinter dieser Zeile (bei einem Titel derselben Ebene hinter seinem Block); unter_titel = ans Ende seiner eigenen Zeilen (vor dem ersten Untertitel), mit am_anfang direkt darunter; sonst ans LV-Ende. oz an einer neuen Zeile = OZ von Hand.
- Nummerierung: Ein fortlaufend nummeriertes LV wird danach neu nummeriert; mit eigenen OZ (z. B. aus GAEB) bekommt nur die neue Zeile eine freie OZ (0015 zwischen 0010 und 0020).
- Werte: Zahlen als JSON-Zahl oder deutscher Text („12,5“, „1.150“ = 1150), höchstens drei Nachkommastellen; Daten TT.MM.JJJJ; Prozent 0–100. Ändern-Werkzeuge ändern nur die angegebenen Felder; null oder „“ leert; Listen ersetzen die ganze Liste ([] = leer). ep = Planerpreis netto.
- status „fertig“ sperrt eine Position; „in Bearbeitung“ im selben Aufruf hebt das auf.

Schreiben
- Jedes Werkzeug mit readOnlyHint false braucht die Freigabe des Anwenders.
- Ist das Projekt in einem proAVA-Fenster offen, arbeitet jedes Werkzeug dort (auch ungespeicherter Stand): eine Änderung ist ein Schritt (Strg+Z), markiert, nicht gespeichert – fenster_speichern. Rückfragen stellt das Fenster dem Anwender („wartet auf den Anwender“).
- Sonst speichern Werkzeuge sofort; vor dem ersten Schreiben je Serverlauf liegt eine Sicherung (Zip) neben dem Projekt. Ist es in einem proAVA offen, das die Brücke nicht erreicht, lehnen sie ab.
- Exporte, PDFs: ziel ohne Angabe neben dem Projekt; Vorhandenes nur mit ueberschreiben. logo: Bilddatei, „“ = ohne.
- bestaetigen (nur ohne Fenster): Kommen in einem LV mit Angeboten oder Auftrag Positionen hinzu oder fallen weg, speichert das Werkzeug nichts und nennt den Grund; nach dem Ja des Anwenders mit bestaetigen: true wiederholen. Zusätzliches bei Auftrag besser als Nachtrag (nachtragsposition_setzen, neu: true).

Langtext (langtext, hinweis, text)
Zeilen = Absätze; **fett**, *kursiv*, __unterstrichen__; „- “ Aufzählung, „1. “ nummeriert, je zwei Leerzeichen davor eine Ebene tiefer; „| a | b |“ Tabelle (Trennzeile „|---|---|“ nach der ersten Zeile = Kopfzeile; Breiten „|--- 15 mm|--- 48 mm|“); \\ vor einem Zeichen = wörtlich, am Zeilenende = Umbruch; {TE 11} = Stelle der Textergänzung 11; [Bild: name, breite mm]. Beispiel: „Liefern:\\n- **DDC-Station** mit Bedienteil\\n- Netzteil 24 V“.

Positionsfelder
- art: bedarf und wahl zählen nicht; wahlgruppe/wahl_nr bei grund und wahl.
- kg: DIN 276, drei Ziffern (481 Automation, 482 Schaltschränke, 484 Kabel); lb: Leistungsbereich („070“ GA oder eigener Name). An Titeln Vorgabe für alles darunter; „“ = die des Titels.
- textergaenzungen: je {kennung: 11, art: „AG“ oder „Bieter“, vortext, inhalt (nur AG), nachtext}; vortext/nachtext wörtlich (Leerzeichen, \\n); ohne {TE n} nach dem Text.
- mengenermittlung: je {ort, rechenansatz: „2*12,5+3“}; ohne menge wird die Summe die Menge.
- zuschlag: {art: alle (Positionen davor) | gekennzeichnet (zu_bezuschlagen) | liste (bezug: [„01.0010“, {oz, teilmenge}]), prozent}; false = normale Position.
- unterbeschreibungen: je {nummer, kurztext, langtext, menge, einheit}.
- Hinweise: vorbemerkung = vor dem LV, schlussbemerkung = nach dem LV (GAEB AddText).

Kopf-/Fußzeile (kopf_links … fuss_rechts; projektangaben_setzen: die des Projekts, pdf_drucken: nur dieser Ausdruck): Text mit Platzhaltern {platzhalter}, z. B. „Seite {seite} von {seiten}“; zweite Zeile nach \\n (kopf_mitte: erste fett); „“ = leer. kopf_logo aus, links oder rechts; kopf_schrift_pt 7–10.
pdf_drucken: kostenermittlung, diagramm, anlagenliste, kostenkontrolle nur für kostenermittlung; ebene, ansicht, brutto, abweichung, vergleich, diagramm nur für kostenverfolgung; sonst Standard der Dokumentart.

Fenster
„links“/„rechts“ (Lage auf dem Bildschirm), „aktiv“ (Standard), Projektname oder PID. Typische Abläufe: (a) „Im rechten Fenster die Position für den Schaltschrank zeigen und links an die Schreibmarke kopieren“: fenster_suchen (rechts) → fenster_zeigen → der Anwender bestätigt → fenster_einfuegen (fenster links, quelle_fenster rechts, auswahl true). (b) „LV aus Projekt xy, such nach …“: fenster_projekt_oeffnen → fenster_suchen → Treffer nennen → „die markierte und die nächsten drei“ = fenster_einfuegen, auswahl true, weitere 3.";

/// [`INSTRUCTIONS`] with the placeholders of Kopf-/Fußzeile.
pub(crate) fn instructions() -> String {
    INSTRUCTIONS.replace("{platzhalter}", &crate::frame_args::placeholder_list())
}

/// The MCP server: one per process, reads and writes project files on demand.
pub struct Server {
    repository: Box<dyn ProjectRepository>,
    exchange: Box<dyn ExchangeFiles>,
    allowed_roots: Vec<PathBuf>,
    backed_up: HashSet<PathBuf>,
    pub(crate) print: PrintDefaults,
    spelling: SpellDefaults,
    libraries: LibraryDefaults,
    templates: TemplateDefaults,
    /// Checker, loaded at the first spelling tool call.
    speller: Option<Result<std::sync::Arc<proava_spell::Speller>, String>>,
    /// Published catalogue lists of AUSSCHREIBEN.DE (`ausschreiben_suchen`).
    web: std::sync::Arc<dyn proava_application::WebTexts>,
    /// proAVA's browser window (`ausschreiben_oeffnen`); none = not set up.
    browser: Option<Box<dyn crate::BrowserWindow>>,
    /// The open proAVA windows (local bridge, `fenster_*`); none = not set up.
    windows: Option<Box<dyn crate::WindowLink>>,
    /// Was the server's program file replaced while it runs (an update)?
    /// None = not checked (tests without the stand-in).
    update_check: Option<Box<dyn Fn() -> bool + Send>>,
    /// The hint on the update was given (once per session).
    update_hinted: bool,
    /// The tool that runs (for the line „KI: …“ in a window).
    tool: String,
    /// What the windows said about changes they took over in this call
    /// (appended to the answer).
    window_notes: Vec<String>,
}

/// Appended once per session to a tool answer when the server's program file
/// was replaced while it runs: it still runs the old code.
pub const UPDATE_HINT: &str =
    "Hinweis: proava-mcp wurde aktualisiert – MCP-Client (z. B. opencode) neu starten";

/// Spelling as set in proAVA (Einstellungen ▸ Darstellung): dictionary
/// folder and personal words; only read.
#[derive(Debug, Clone, Default)]
pub struct SpellDefaults {
    pub dictionary_folder: Option<PathBuf>,
    pub personal_words: Vec<String>,
}

/// Libraries as set in proAVA (Einstellungen ▸ Neue Projekte): the own one
/// and further ones; only read.
#[derive(Debug, Clone, Default)]
pub struct LibraryDefaults {
    /// Own library; None = ~/proAVA-Bibliothek.proava.
    pub own: Option<PathBuf>,
    pub others: Vec<proava_application::LibrarySetting>,
}

/// Projektvorlagen as in proAVA (Einstellungen ▸ Neue Projekte): the
/// folder of the own templates and the defaults of a new LV; only read.
#[derive(Debug, Clone, Default)]
pub struct TemplateDefaults {
    /// Folder of the own templates (None = none known).
    pub folder: Option<PathBuf>,
    /// OZ-Maske, Schrittweite, MwSt and planner of new projects.
    pub preferences: proava_application::Preferences,
}

/// Print settings taken from proAVA's Einstellungen (Logo).
#[derive(Debug, Clone, Default)]
pub struct PrintDefaults {
    pub logo_file: Option<PathBuf>,
    /// Kopf-/Fußzeile from Extras ▸ Einstellungen ▸ Drucken.
    pub page_frame: proava_domain::PageFrame,
    /// Blocks of the Deckblatt from the settings.
    pub cover_blocks: proava_application::CoverBlocks,
    /// Druckstil (default) and accent colour from the settings.
    pub look: proava_application::PrintLook,
    /// Bearbeiter from the settings.
    pub author: String,
}

/// A project read from disk together with what the file looked like then.
pub(crate) struct Loaded {
    pub path: PathBuf,
    /// The LV chosen with the argument `lv` is the active one here.
    pub project: Project,
    stamp: Option<(Option<SystemTime>, u64)>,
    /// The LV proAVA shows when opening the file; saving keeps it.
    stored_active: usize,
    /// Position ids of every LV (by session key) as read: positions that a
    /// tool adds or removes in an LV with bids or Auftrag need `bestaetigen`.
    positions_before: Vec<(u64, std::collections::BTreeSet<proava_domain::RowId>)>,
    /// `bestaetigen: true` – the user said yes to such a change.
    confirmed: bool,
    /// Open in a window: the tool worked on its state, saving hands the
    /// change back to the window.
    window: Option<crate::window_tools::WindowState>,
    /// Open in a window where assistants may not work: read from the file,
    /// no change (the text says why).
    switched_off: Option<String>,
}

impl Loaded {
    pub(crate) fn stored_active(&self) -> usize {
        self.stored_active
    }
}

impl Server {
    pub fn new(repository: Box<dyn ProjectRepository>, exchange: Box<dyn ExchangeFiles>) -> Self {
        Self {
            repository,
            exchange,
            allowed_roots: Vec::new(),
            backed_up: HashSet::new(),
            print: PrintDefaults::default(),
            spelling: SpellDefaults::default(),
            libraries: LibraryDefaults::default(),
            templates: TemplateDefaults::default(),
            speller: None,
            web: std::sync::Arc::new(proava_application::NoWeb),
            browser: None,
            windows: None,
            update_check: None,
            update_hinted: false,
            tool: String::new(),
            window_notes: Vec::new(),
        }
    }

    /// How the server sees an update of its program file (the composition
    /// root reads the file, tests hand in a switch).
    pub fn with_update_check(mut self, check: Box<dyn Fn() -> bool + Send>) -> Self {
        self.update_check = Some(check);
        self
    }

    /// The answer of a tool, with [`UPDATE_HINT`] the first time an update
    /// is seen in this session.
    fn with_update_hint(&mut self, text: String) -> String {
        if self.update_hinted || !self.update_check.as_ref().is_some_and(|check| check()) {
            return text;
        }
        self.update_hinted = true;
        format!("{text}\n\n{UPDATE_HINT}")
    }

    /// The open proAVA windows for the `fenster_*` tools (the composition
    /// root hands in the real folder, tests a recording stand-in).
    pub fn with_windows(mut self, windows: Box<dyn crate::WindowLink>) -> Self {
        self.windows = Some(windows);
        self
    }

    pub(crate) fn window_link(&self) -> Option<&dyn crate::WindowLink> {
        self.windows.as_deref()
    }

    /// Web access for the catalogue lists (the composition root hands in curl
    /// with cache, tests a stand-in).
    pub fn with_web(mut self, web: std::sync::Arc<dyn proava_application::WebTexts>) -> Self {
        self.web = web;
        self
    }

    pub(crate) fn web_texts(&self) -> &dyn proava_application::WebTexts {
        self.web.as_ref()
    }

    /// proAVA's browser window for `ausschreiben_oeffnen` (only the composition
    /// root starts a real one; tests never do).
    pub fn with_browser_window(mut self, window: Box<dyn crate::BrowserWindow>) -> Self {
        self.browser = Some(window);
        self
    }

    pub(crate) fn browser_window(&mut self) -> Option<&mut (dyn crate::BrowserWindow + 'static)> {
        self.browser.as_deref_mut()
    }

    /// Libraries as in proAVA (the composition root reads them from the settings).
    pub fn with_library_defaults(mut self, libraries: LibraryDefaults) -> Self {
        self.libraries = libraries;
        self
    }

    /// The own library as set in proAVA, else the default file in the
    /// personal folder, as in the program (`%USERPROFILE%` on Windows).
    pub(crate) fn own_library(&self) -> PathBuf {
        self.libraries.own.clone().unwrap_or_else(|| {
            proava_infrastructure::paths::home_dir().join("proAVA-Bibliothek.proava")
        })
    }

    /// The own library first, then the further ones.
    pub(crate) fn libraries(&self) -> Vec<proava_application::LibrarySource> {
        let own = self.own_library();
        let mut list = vec![proava_application::LibrarySource {
            path: own.clone(),
            label: "Eigene Bibliothek".to_string(),
            read_only: false,
            own: true,
        }];
        for other in &self.libraries.others {
            if other.path == own || list.iter().any(|known| known.path == other.path) {
                continue;
            }
            list.push(proava_application::LibrarySource {
                path: other.path.clone(),
                label: if other.label.trim().is_empty() {
                    proava_application::library::library_name(&other.path)
                } else {
                    other.label.trim().to_string()
                },
                read_only: other.read_only,
                own: false,
            });
        }
        list
    }

    /// Template folder and new-project defaults as in proAVA.
    pub fn with_template_defaults(mut self, templates: TemplateDefaults) -> Self {
        self.templates = templates;
        self
    }

    pub(crate) fn template_defaults(&self) -> &TemplateDefaults {
        &self.templates
    }

    pub fn with_spell_defaults(mut self, spelling: SpellDefaults) -> Self {
        self.spelling = spelling;
        self
    }

    /// A ready checker instead of the system dictionary (tests).
    pub fn with_speller(mut self, speller: proava_spell::Speller) -> Self {
        self.speller = Some(Ok(std::sync::Arc::new(speller)));
        self
    }

    /// The German checker (loaded once).
    pub(crate) fn speller(&mut self) -> Result<std::sync::Arc<proava_spell::Speller>, String> {
        if self.speller.is_none() {
            self.speller = Some(
                proava_spell::Speller::find_and_load(
                    self.spelling.dictionary_folder.as_deref(),
                    &self.spelling.personal_words,
                )
                .map(std::sync::Arc::new)
                .map_err(|error| error.to_string()),
            );
        }
        self.speller.clone().expect("loaded above")
    }

    /// Logo and cover sheet as in proAVA (the composition root reads them
    /// from the settings).
    pub fn with_print_defaults(mut self, print: PrintDefaults) -> Self {
        self.print = print;
        self
    }

    /// Logo for printouts: the tool's `logo` (path, "" = none) or the settings' logo.
    pub(crate) fn pdf_logo(
        &self,
        args: &Args,
    ) -> Result<Option<proava_application::PdfLogo>, String> {
        let logo_file = match args.opt_text("logo")? {
            Some(text) if text.trim().is_empty() => None,
            Some(text) => Some(self.allowed(Path::new(text.trim()))?),
            None => self.print.logo_file.clone(),
        };
        logo_file
            .map(|file| {
                Ok(proava_application::PdfLogo {
                    file_name: file
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| "logo.png".into()),
                    bytes: fs::read(&file).map_err(|error| {
                        format!("Logo {} nicht lesbar: {error}", file.display())
                    })?,
                })
            })
            .transpose()
    }

    /// Only files below these folders may be read or written (empty = all).
    pub fn with_allowed_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.allowed_roots = roots
            .into_iter()
            .map(|root| real_path(&root).unwrap_or(root))
            .collect();
        self
    }

    /// Handles one JSON-RPC line; returns the answer line, if any.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let answer = match serde_json::from_str::<Value>(line) {
            Ok(message) => self.handle(&message),
            Err(error) => Some(error_answer(
                Value::Null,
                -32700,
                &format!("Parse error: {error}"),
            )),
        };
        answer.map(|value| value.to_string())
    }

    /// Handles one JSON-RPC message; notifications get no answer.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return id.map(|id| error_answer(id, -32600, "Invalid request"));
        };
        let id = id?;
        let empty = Map::new();
        let params = message
            .get("params")
            .and_then(Value::as_object)
            .unwrap_or(&empty);

        let result = match method {
            "initialize" => {
                let wanted = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .filter(|version| KNOWN_PROTOCOLS.contains(version))
                    .unwrap_or(PROTOCOL_VERSION);
                json!({
                    "protocolVersion": wanted,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "proava", "title": "proAVA", "version": SERVER_VERSION},
                    "instructions": instructions(),
                })
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools": catalog::tools()}),
            "tools/call" => {
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return Some(error_answer(id, -32602, "Werkzeugname fehlt."));
                };
                let arguments = params
                    .get("arguments")
                    .and_then(Value::as_object)
                    .unwrap_or(&empty);
                let (text, is_error) = match self.call_tool(name, arguments) {
                    Ok(text) => (text, false),
                    Err(text) => (text, true),
                };
                let text = self.with_update_hint(text);
                json!({"content": [{"type": "text", "text": text}], "isError": is_error})
            }
            _ => {
                return Some(error_answer(
                    id,
                    -32601,
                    &format!("Unbekannte Methode: {method}"),
                ))
            }
        };
        Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    /// Runs one tool; `Err` becomes an answer with `isError: true`. What a
    /// window said about a change it took over is appended.
    pub fn call_tool(
        &mut self,
        name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<String, String> {
        self.tool = name.to_string();
        self.window_notes.clear();
        let result = self.dispatch(name, arguments);
        let notes = std::mem::take(&mut self.window_notes);
        result.map(|text| {
            if notes.is_empty() {
                text
            } else {
                format!("{text}\n{}", notes.join("\n"))
            }
        })
    }

    fn dispatch(&mut self, name: &str, arguments: &Map<String, Value>) -> Result<String, String> {
        let args = Args(arguments);
        // A Kosten-LV never goes to bidders: tools of the tender refuse it.
        crate::cost_lv_tools::refuse_for_cost_lv(self, name, &args)?;
        match name {
            "projekte_suchen" => read_tools::find_projects(self, &args),
            "projekt_lesen" => read_tools::read_project(self, &args),
            "bieter_lesen" => crate::bidder_tools::read_bidders(self, &args),
            "bieter_anlegen" => crate::bidder_tools::add_bidder(self, &args),
            "bieter_aendern" => crate::bidder_tools::change_bidder(self, &args),
            "submission_drucken" => crate::bidder_tools::print_submission(self, &args),
            "anschreiben_drucken" => crate::bidder_tools::print_letters(self, &args),
            "kosten_lesen" => crate::cost_tools::read_costs(self, &args),
            "kosten_anlegen" => crate::cost_tools::create_cost_estimate(self, &args),
            "kosten_aendern" => crate::cost_tools::change_cost_estimate(self, &args),
            "kosten_zeilen_schreiben" => crate::cost_tools::write_cost_rows(self, &args),
            "kostenverfolgung_lesen" => crate::tracking_tools::read_tracking(self, &args),
            "kostenvorgabe_setzen" => crate::tracking_tools::set_budget(self, &args),
            "kostenverfolgung_aendern" => crate::tracking_tools::change_tracking(self, &args),
            "schriftverkehr_lesen" => crate::correspondence_tools::read_correspondence(self, &args),
            "schriftverkehr_pdf_speichern" => {
                crate::correspondence_tools::save_correspondence_pdf(self, &args)
            }
            "lvs_lesen" => crate::lv_tools::read_lvs(self, &args),
            "lv_anlegen" => crate::lv_tools::create_lv(self, &args),
            "lv_lesen" => read_tools::read_lv(self, &args),
            "position_lesen" => read_tools::read_position(self, &args),
            "lv_pruefen" => read_tools::check_lv(self, &args),
            "rechtschreibung_pruefen" => read_tools::check_spelling(self, &args),
            "kostengruppen_lesen" => read_tools::read_cost_groups(self, &args),
            "angebote_lesen" => read_tools::read_bids(self, &args),
            "preisspiegel_lesen" => read_tools::read_comparison(self, &args),
            "projekt_anlegen" => write_tools::create_project(self, &args),
            "projektangaben_setzen" => write_tools::set_project_info(self, &args),
            "titel_einfuegen" => write_tools::insert_heading(self, &args),
            "position_einfuegen" => write_tools::insert_position(self, &args),
            "positionen_einfuegen" => write_tools::insert_rows(self, &args),
            "stlb_uebernehmen" => crate::stlb_tools::take_over(self, &args),
            "stlb_pruefdatei" => crate::stlb_tools::check_file(self, &args),
            "stlb_aktualisieren" => crate::stlb_tools::update(self, &args),
            "ausschreiben_suchen" => crate::ausschreiben_tools::search(self, &args),
            "ausschreiben_oeffnen" => crate::ausschreiben_tools::open(self, &args),
            "lv_suchen" => crate::search_tools::search(self, &args),
            "fenster_lesen" => crate::window_tools::read(self, &args),
            "fenster_zeigen" => crate::window_tools::show(self, &args),
            "fenster_suchen" => crate::window_tools::search(self, &args),
            "fenster_einfuegen" => crate::window_tools::insert(self, &args),
            "fenster_projekt_oeffnen" => crate::window_tools::open_project(self, &args),
            "fenster_speichern" => crate::window_tools::save(self, &args),
            "positionen_uebernehmen" => crate::transfer_tools::take_over(self, &args),
            "lv_vergleichen" => crate::compare_tools::compare_lv(self, &args),
            "bibliothek_lesen" => crate::transfer_tools::read_library(self, &args),
            "bibliothek_suchen" => crate::library_tools::search(self, &args),
            "bibliothek_einfuegen" => crate::library_tools::insert(self, &args),
            "bibliothek_ablegen" => crate::library_tools::store(self, &args),
            "bibliothek_abgleichen" => crate::library_tools::compare(self, &args),
            "hinweis_einfuegen" => write_tools::insert_note(self, &args),
            "abstand_einfuegen" => write_tools::insert_spacer(self, &args),
            "ausfuehrungsbeschreibung_einfuegen" => write_tools::insert_performance(self, &args),
            "bild_einfuegen" => write_tools::insert_image(self, &args),
            "bieterangabe_einfuegen" => write_tools::insert_bidder_field(self, &args),
            "pdf_ausschnitt_einfuegen" => write_tools::insert_pdf_clip(self, &args),
            "zeile_aendern" => write_tools::change_row(self, &args),
            "zeile_loeschen" => write_tools::delete_row(self, &args),
            "zeile_umwandeln" => write_tools::convert_row(self, &args),
            "zeile_verschieben" => write_tools::move_row(self, &args),
            "neu_nummerieren" => write_tools::renumber(self, &args),
            "angebot_nachlass_setzen" => write_tools::set_discount(self, &args),
            "auftrag_erteilen" => write_tools::award(self, &args),
            "preise_uebernehmen" => write_tools::adopt_prices(self, &args),
            "auftrags_lv_exportieren" => exchange_tools::export_contract(self, &args),
            "gaeb_exportieren" => exchange_tools::export_gaeb(self, &args),
            "angebot_einlesen" => exchange_tools::import_bid(self, &args),
            "lv_aus_gaeb" => exchange_tools::import_lv(self, &args),
            "gaeb_importieren" => exchange_tools::import_gaeb(self, &args),
            "csv_importieren" => exchange_tools::import_csv(self, &args),
            "csv_exportieren" => exchange_tools::export_csv(self, &args),
            "pdf_drucken" => exchange_tools::print_pdf(self, &args),
            "abrechnung_lesen" => crate::billing_tools::read_billing(self, &args),
            "freigabe_berechnen" => crate::billing_tools::release(self, &args),
            "auftrag_festschreiben" => crate::billing_tools::fix_contract(self, &args),
            "rechnung_anlegen" => crate::billing_tools::create_invoice(self, &args),
            "rechnung_aendern" => crate::billing_tools::change_invoice(self, &args),
            "rechnungsposition_setzen" => crate::billing_tools::set_position(self, &args),
            "aufmass_setzen" => crate::billing_tools::set_takeoff(self, &args),
            "nachtraege_lesen" => crate::change_order_tools::read(self, &args),
            "nachtrag_anlegen" => crate::change_order_tools::create(self, &args),
            "nachtrag_aendern" => crate::change_order_tools::change(self, &args),
            "nachtragsposition_setzen" => crate::change_order_tools::set_item(self, &args),
            "nachtrag_status" => crate::change_order_tools::set_status(self, &args),
            "nachtrag_drucken" => crate::change_order_tools::print(self, &args),
            "nachtrag_gaeb_exportieren" => crate::change_order_tools::export(self, &args),
            "nachtragsangebot_einlesen" => crate::change_order_tools::import_offer(self, &args),
            _ => Err(format!("Unbekanntes Werkzeug: {name}")),
        }
    }

    pub(crate) fn repository(&self) -> &dyn ProjectRepository {
        self.repository.as_ref()
    }

    pub(crate) fn exchange(&self) -> &dyn ExchangeFiles {
        self.exchange.as_ref()
    }

    /// Checks that a path lies inside the allowed folders.
    pub(crate) fn allowed(&self, path: &Path) -> Result<PathBuf, String> {
        if path.as_os_str().is_empty() {
            return Err("Der Pfad ist leer.".to_string());
        }
        if !path.is_absolute() {
            return Err(format!(
                "Bitte einen absoluten Pfad angeben: {}",
                path.display()
            ));
        }
        if self.allowed_roots.is_empty() {
            return Ok(path.to_path_buf());
        }
        let mut existing = path.to_path_buf();
        let mut rest = Vec::new();
        let resolved = loop {
            if let Ok(real) = real_path(&existing) {
                break rest
                    .iter()
                    .rev()
                    .fold(real, |path: PathBuf, part| path.join(part));
            }
            match (existing.file_name(), existing.parent()) {
                (Some(name), Some(parent)) => {
                    rest.push(name.to_os_string());
                    existing = parent.to_path_buf();
                }
                _ => return Err(format!("Pfad nicht lesbar: {}", path.display())),
            }
        };
        if self
            .allowed_roots
            .iter()
            .any(|root| resolved.starts_with(root))
        {
            Ok(resolved)
        } else {
            Err(format!(
                "{} liegt außerhalb der freigegebenen Ordner ({}).",
                path.display(),
                self.allowed_roots
                    .iter()
                    .map(|root| root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }

    /// Loads the project named in `projekt`; the LV named in `lv` (number or
    /// position, see `lv_tools::select_lv`) becomes the active one for the tool.
    pub(crate) fn open(&self, args: &Args) -> Result<Loaded, String> {
        let path = PathBuf::from(args.text("projekt")?.trim());
        let mut loaded = self.open_path(&path)?;
        let index = crate::lv_tools::select_lv(&loaded.project, args.get("lv"))?;
        loaded.project.set_active_lv(index);
        loaded.confirmed = args.flag("bestaetigen")?;
        Ok(loaded)
    }

    pub(crate) fn open_path(&self, path: &Path) -> Result<Loaded, String> {
        let path = self.allowed(path)?;
        let is_project = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(PROJECT_EXTENSION));
        if !is_project {
            return Err(format!(
                "Keine proAVA-Projektdatei (.proava): {}",
                path.display()
            ));
        }
        if !path.is_file() {
            return Err(format!("Projektdatei nicht gefunden: {}", path.display()));
        }
        // Open in a window: the tool works on the window's state.
        let mut window = None;
        let mut switched_off = None;
        let mut from_window = None;
        if let Some(entry) = crate::window_tools::window_with(self, &path) {
            match crate::window_tools::window_state(self, &entry, &path)? {
                crate::window_tools::FromWindow::State(project, state) => {
                    from_window = Some(*project);
                    window = Some(*state);
                }
                crate::window_tools::FromWindow::Off(text) => switched_off = Some(text),
            }
        }
        let stamp = file_stamp(&path);
        let project = match from_window {
            Some(project) => project,
            None => self.repository.load_project(&path)?,
        };
        let stored_active = project.active_lv();
        let positions_before = (0..project.lv_count())
            .filter_map(|index| {
                let key = project.lv_key(index)?;
                let lv = project.lv_at(index)?;
                Some((key, proava_domain::position_ids(lv)))
            })
            .collect();
        Ok(Loaded {
            path,
            project,
            stamp,
            stored_active,
            positions_before,
            confirmed: false,
            window,
            switched_off,
        })
    }

    /// Saves a loaded project unless the file changed meanwhile. The first
    /// write per run and file makes a backup next to it; its path is returned.
    /// Open in a window: the change goes back to the window instead (one
    /// undo step there, nothing saved; its questions appear there).
    pub(crate) fn save(&mut self, loaded: &Loaded) -> Result<Option<PathBuf>, String> {
        if let Some(text) = &loaded.switched_off {
            return Err(text.clone());
        }
        if let Some(window) = &loaded.window {
            let note = crate::window_tools::adopt(self, &loaded.project, window, &self.tool)?;
            self.window_notes.push(note);
            return Ok(None);
        }
        // Open in a proAVA the bridge does not reach (another computer,
        // another user, an older proAVA): it would overwrite the file when
        // saving. A lock left by a proAVA that no longer runs does not count.
        if let Some(lock) = self
            .repository
            .read_lock(&loaded.path)
            .filter(|lock| !lock.own && !lock.stale)
        {
            return Err(format!(
                "Nichts gespeichert: {} ist in proAVA geöffnet von {} – dieses Fenster erreicht die \
                 KI-Brücke nicht (anderer Rechner, anderer Benutzer oder älteres proAVA), und es würde \
                 die Änderung beim Speichern überschreiben. Der Anwender speichert und schließt das Projekt dort, \
                 dann noch einmal.",
                loaded.path.display(),
                lock.who()
            ));
        }
        if let Some(question) = uncovered_question(loaded) {
            return Err(question);
        }
        if file_stamp(&loaded.path) != loaded.stamp {
            return Err(format!(
                "Nichts gespeichert: {} wurde inzwischen geändert (proAVA oder ein anderes \
                 Werkzeug). Bitte neu lesen und die Änderung wiederholen.",
                loaded.path.display()
            ));
        }
        let backup = self
            .backup_once(&loaded.path)
            .map_err(|error| format!("Nichts gespeichert, {error}"))?;
        if loaded.project.active_lv() == loaded.stored_active {
            self.repository
                .save_project(&loaded.path, &loaded.project)?;
        } else {
            // Das in proAVA aktive LV bleibt, auch wenn ein anderes bearbeitet wurde.
            let mut project = loaded.project.clone();
            project.set_active_lv(loaded.stored_active);
            self.repository.save_project(&loaded.path, &project)?;
        }
        Ok(backup)
    }

    /// The backup before the first write of `path` in this server run
    /// (next to it); None when there was one already.
    pub(crate) fn backup_once(&mut self, path: &Path) -> Result<Option<PathBuf>, String> {
        if self.backed_up.contains(path) || !path.is_file() {
            return Ok(None);
        }
        let folder = path.parent().unwrap_or(Path::new("/"));
        let file = self
            .repository
            .backup(path, folder)
            .map_err(|error| format!("Sicherung fehlgeschlagen: {error}"))?;
        self.backed_up.insert(path.to_path_buf());
        Ok(Some(file))
    }

    /// Writes a project that did not exist before (no backup needed).
    pub(crate) fn save_new(&mut self, path: &Path, project: &Project) -> Result<(), String> {
        self.repository.save_project(path, project)?;
        self.backed_up.insert(path.to_path_buf());
        Ok(())
    }

    /// Saves and adds the backup note to an answer.
    pub(crate) fn save_with_answer(
        &mut self,
        loaded: &Loaded,
        answer: String,
    ) -> Result<String, String> {
        Ok(match self.save(loaded)? {
            Some(backup) => format!(
                "{answer}\nSicherung vor der ersten Änderung: {}",
                backup.display()
            ),
            None => answer,
        })
    }
}

/// Positions added to or removed from an LV that has bids or an Auftrag
/// (Anwender 19.11.): without `bestaetigen: true` nothing is saved and the
/// answer names the reason – like the question in the proAVA window.
fn uncovered_question(loaded: &Loaded) -> Option<String> {
    if loaded.confirmed {
        return None;
    }
    let project = &loaded.project;
    let several = project.lv_count() > 1;
    let mut notes = Vec::new();
    let mut order = false;
    for index in 0..project.lv_count() {
        let Some(key) = project.lv_key(index) else {
            continue;
        };
        let Some((_, before)) = loaded
            .positions_before
            .iter()
            .find(|(known, _)| *known == key)
        else {
            continue;
        };
        let uncovered = project.uncovered_positions(index, before);
        let note = if several {
            let name = crate::lv_tools::label(project.lv_at(index)?);
            project.uncovered_note_for(index, &uncovered, &name, "gelöschte")
        } else {
            project.uncovered_note(index, &uncovered)
        };
        if let Some(note) = note {
            order |= project.bids_or_order(index).order;
            notes.push(note);
        }
    }
    if notes.is_empty() {
        return None;
    }
    let mut text = format!("Rückfrage – nichts gespeichert: {}", notes.join(" "));
    if order {
        text.push_str(
            " Der Weg über den Nachtrag fragt nicht: nachtrag_anlegen, dann \
             nachtragsposition_setzen mit neu: true.",
        );
    }
    text.push_str(" Erst den Anwender fragen; mit seinem Ja mit bestaetigen: true wiederholen.");
    Some(text)
}

fn file_stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    fs::metadata(path)
        .ok()
        .map(|metadata| (metadata.modified().ok(), metadata.len()))
}

/// The real path of an existing file or folder (links resolved), written as
/// Windows users know it: `fs::canonicalize` gives `\\?\C:\…` and
/// `\\?\UNC\server\…` there, which would show in the answers and in paths
/// kept in projects and the library. Long paths still work, Rust adds the
/// prefix itself where it is needed.
pub fn real_path(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path).map(|real| without_verbatim_prefix(real, cfg!(windows)))
}

/// `\\?\C:\…` → `C:\…`, `\\?\UNC\server\…` → `\\server\…` (only on Windows).
/// Names that only the long form can carry (ending in a dot or a space)
/// keep it.
fn without_verbatim_prefix(path: PathBuf, windows: bool) -> PathBuf {
    let Some(text) = path.to_str().filter(|_| windows) else {
        return path;
    };
    let plain = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || bytes[2] != b'\\'
        {
            return path;
        }
        rest.to_string()
    } else {
        return path;
    };
    if plain
        .split('\\')
        .any(|part| part.ends_with(['.', ' ']) && part != "." && part != "..")
    {
        return path;
    }
    PathBuf::from(plain)
}

fn error_answer(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// Reads newline-delimited JSON-RPC from stdin and answers on stdout.
pub fn run_stdio(mut server: Server) -> io::Result<()> {
    eprintln!("[proava-mcp] proAVA-MCP {SERVER_VERSION} gestartet");
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(answer) = server.handle_line(&line) {
            stdout.write_all(answer.as_bytes())?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use proava_application::NoExchange;
    use proava_infrastructure::XmlProjectFile;

    use super::Server;

    #[test]
    fn windows_paths_without_the_long_prefix() {
        use std::path::PathBuf;
        let plain = |text: &str| super::without_verbatim_prefix(PathBuf::from(text), true);
        assert_eq!(
            plain(r"\\?\C:\Users\Jörg Müller\LV 1.proava"),
            PathBuf::from(r"C:\Users\Jörg Müller\LV 1.proava")
        );
        assert_eq!(
            plain(r"\\?\UNC\server\Projekte\LV.proava"),
            PathBuf::from(r"\\server\Projekte\LV.proava")
        );
        assert_eq!(plain(r"C:\a"), PathBuf::from(r"C:\a"));
        assert_eq!(
            plain(r"\\?\Volume{1234}\a"),
            PathBuf::from(r"\\?\Volume{1234}\a"),
            "kein Laufwerk: bleibt"
        );
        assert_eq!(
            plain(r"\\?\C:\Ordner.\a"),
            PathBuf::from(r"\\?\C:\Ordner.\a"),
            "nur in Langform gültig"
        );
        assert_eq!(
            plain(r"\\?\C:\Ordner \a"),
            PathBuf::from(r"\\?\C:\Ordner \a")
        );
        assert_eq!(
            super::without_verbatim_prefix(PathBuf::from(r"\\?\C:\a"), false),
            PathBuf::from(r"\\?\C:\a"),
            "außer Windows nichts"
        );
    }

    #[test]
    fn a_file_changed_after_loading_is_not_overwritten() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-mcp-stamp-")
            .tempdir()
            .unwrap();
        let folder = scratch.path().to_path_buf();
        std::fs::create_dir_all(&folder).unwrap();
        let mut server = Server::new(Box::new(XmlProjectFile::new()), Box::new(NoExchange));
        let path = server
            .repository()
            .create_project(&folder, &proava_domain::ProjectName::parse("P").unwrap())
            .unwrap();
        let loaded = server.open_path(&path).unwrap();
        let mut changed = loaded.project.clone();
        changed.lv_mut().set_title("Von proAVA");
        std::thread::sleep(std::time::Duration::from_millis(20));
        server.repository().save_project(&path, &changed).unwrap();

        let error = server.save(&loaded).unwrap_err();
        assert!(error.contains("inzwischen geändert"), "{error}");
        let on_disk = server.open_path(&path).unwrap();
        assert_eq!(on_disk.project.lv().title(), "Von proAVA");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
