//! What the running proAVA windows and the MCP server agree on: the local
//! bridge („Brücke“).
//!
//! Every proAVA window (only the real program, never a test) opens a local
//! channel for the own user – a Unix socket `<pid>.sock` in a folder only the
//! user can enter, on Windows a named pipe only the user may open – and
//! announces itself in the same folder with an entry `<pid>.json`
//! ([`WindowEntry`]). The MCP server reads the entries, asks a window with a
//! [`Request`] and gets one [`Response`]; both are one JSON object per line.
//!
//! An assistant works *in* the open windows this way instead of in the files:
//! changes run through the normal editing of the program (one Strg+Z takes
//! them back, no clash with unsaved changes), and the user sees what happens.
//!
//! Everything here is pure – no socket, no file, no window – so it is tested
//! on its own; the channel lives in `proava-infrastructure`, the handling of
//! the commands in `proava-ui`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Version of the protocol; a window answers a request of another version
/// with [`Reply::Refused`] and names both.
pub const VERSION: u32 = 1;

/// Longest line that is read: rows with images are handed over as an
/// proAVA clipboard text (base64, at most about 32 MB).
pub const MAX_LINE: usize = 64 * 1024 * 1024;

/// Subfolder of the bridge folder where whole projects pass between the MCP
/// server and a window ([`Command::Stand`], [`Command::Uebernehmen`]): only
/// the own user can enter it, like the folder itself.
pub const EXCHANGE_FOLDER: &str = "austausch";

/// File name of the entry of a window in the folder.
pub fn entry_file_name(pid: u32) -> String {
    format!("{pid}.json")
}

/// File name of the socket of a window in the folder (Unix).
pub fn socket_file_name(pid: u32) -> String {
    format!("{pid}.sock")
}

/// PID of an entry file name („4711.json“ → 4711), else None.
pub fn pid_of_entry(name: &str) -> Option<u32> {
    pid_with_suffix(name, ".json")
}

/// PID of a socket file name („4711.sock“ → 4711), else None.
pub fn pid_of_socket(name: &str) -> Option<u32> {
    pid_with_suffix(name, ".sock")
}

fn pid_with_suffix(name: &str, suffix: &str) -> Option<u32> {
    let digits = name.strip_suffix(suffix)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

// ----- Anfragen -----

/// One request of the MCP server to a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Protocol version of the sender ([`VERSION`]).
    pub v: u32,
    /// Number of the request; the response carries it back.
    pub id: u64,
    pub befehl: Command,
}

impl Request {
    pub fn new(id: u64, command: Command) -> Self {
        Self {
            v: VERSION,
            id,
            befehl: command,
        }
    }
}

/// What the window is to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum Command {
    /// The state of the window (project, LV, selection, place on the screen).
    FensterLesen,
    /// Select rows, scroll to them, open their titles, optionally open the
    /// position window – changes nothing in the LV.
    Zeigen(Show),
    /// The KI-Markierung: a frame of its own colour around the rows the
    /// assistant works on, and a line in the status bar.
    Markieren(Mark),
    /// Rows of the selection or from a row on.
    Lesen(Read),
    /// Rows as proAVA clipboard text, to be inserted into another window –
    /// the same data as Strg+C between two windows.
    Kopieren(Take),
    /// Inserts rows (proAVA clipboard text) at the cursor of the window –
    /// the same way as Strg+V, one Strg+Z takes it back.
    Einfuegen(Insert),
    /// Opens a project: in this window if it shows the start page, else in a
    /// new window.
    ProjektOeffnen(OpenProject),
    /// The search of „Übernehmen aus Projekt“ in the window's LV; the hits
    /// are selected and marked.
    Suchen(Search),
    /// Rows from another proAVA window are dragged over this one (the user's
    /// own drag between windows, not the assistant): show the insert mark.
    ZiehenUeber(DragOver),
    /// The drag left this window or was cancelled: the mark goes.
    ZiehenEnde { kennung: u64 },
    /// The rows were let go over this window: insert them at the mark (a
    /// copy, the other window stays as it is).
    Ablegen(DragDrop),
    /// The whole project as the window has it now (also unsaved), written
    /// to a file in the exchange folder: an MCP tool works on it like on the
    /// project file.
    Stand,
    /// The project changed by an MCP tool on a [`Command::Stand`]: the
    /// window takes it over as one undo step, marks what changed and asks
    /// the user where a person at the keys would be asked. Nothing is saved.
    Uebernehmen(Adopt),
    /// Saves the project like Strg+S.
    Speichern,
}

impl Command {
    /// Name for messages („zeigen“, „einfuegen“ …).
    pub fn name(&self) -> &'static str {
        match self {
            Command::FensterLesen => "fenster_lesen",
            Command::Zeigen(_) => "zeigen",
            Command::Markieren(_) => "markieren",
            Command::Lesen(_) => "lesen",
            Command::Kopieren(_) => "kopieren",
            Command::Einfuegen(_) => "einfuegen",
            Command::ProjektOeffnen(_) => "projekt_oeffnen",
            Command::Suchen(_) => "suchen",
            Command::ZiehenUeber(_) => "ziehen_ueber",
            Command::ZiehenEnde { .. } => "ziehen_ende",
            Command::Ablegen(_) => "ablegen",
            Command::Stand => "stand",
            Command::Uebernehmen(_) => "uebernehmen",
            Command::Speichern => "speichern",
        }
    }

    /// Whether the command belongs to dragging between windows – that is
    /// The user's own work and goes even when assistants may not work in the
    /// window.
    pub fn is_drag(&self) -> bool {
        matches!(
            self,
            Command::ZiehenUeber(_) | Command::ZiehenEnde { .. } | Command::Ablegen(_)
        )
    }

    /// Whether the command changes the project in the window.
    pub fn changes_project(&self) -> bool {
        matches!(
            self,
            Command::Einfuegen(_) | Command::Ablegen(_) | Command::Uebernehmen(_)
        )
    }
}

/// Which rows of an LV: the selection, OZ, row ids, and the rows after them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rows {
    /// LV number („2“, „481“) or place in the LV list (1 = first); None = the
    /// LV the window shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lv: Option<String>,
    /// The rows selected in the window (also when OZ or ids are given: added).
    #[serde(default, skip_serializing_if = "is_false")]
    pub auswahl: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oz: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<u64>,
    /// So many further rows after the last chosen one (positions, hints,
    /// Ausführungsbeschreibungen; titles and spacers are skipped) –
    /// „die markierte und die nächsten drei“.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weitere: usize,
}

impl Rows {
    /// The selection of the window.
    pub fn selection() -> Self {
        Self {
            auswahl: true,
            ..Self::default()
        }
    }

    /// Nothing named: no selection, OZ or ids.
    pub fn is_empty(&self) -> bool {
        !self.auswahl && self.oz.is_empty() && self.ids.is_empty()
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// [`Command::Zeigen`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Show {
    pub zeilen: Rows,
    /// Also open the window „Eigenschaften“ of the first row.
    #[serde(default, skip_serializing_if = "is_false")]
    pub positionsfenster: bool,
    /// Text of the KI-Markierung; None = „KI zeigt …“.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markierung: Option<String>,
}

/// [`Command::Markieren`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub zeilen: Rows,
    /// Line in the status bar after „KI: “.
    #[serde(default)]
    pub text: String,
    /// How long the mark stays (seconds; None = [`MARK_SECONDS`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sekunden: Option<u32>,
    /// Take the mark away instead.
    #[serde(default, skip_serializing_if = "is_false")]
    pub aufheben: bool,
}

/// How long a KI-Markierung stays without further requests.
pub const MARK_SECONDS: u32 = 12;
/// Longest time a KI-Markierung may be asked to stay.
pub const MARK_SECONDS_MAX: u32 = 600;

/// [`Command::Lesen`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Read {
    /// Empty: the selection.
    pub zeilen: Rows,
    /// From this OZ (or `#id`) on instead of `zeilen`, `anzahl` rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anzahl: Option<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub langtext: bool,
}

/// [`Command::Kopieren`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Take {
    pub zeilen: Rows,
    /// Quantities come along (as Strg+C).
    #[serde(default = "yes")]
    pub mengen: bool,
    /// Unit prices come along (as Strg+C).
    #[serde(default = "yes")]
    pub preise: bool,
    /// KI-Markierung on the copied rows (text), None = „KI kopiert …“.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markierung: Option<String>,
}

fn yes() -> bool {
    true
}

impl Default for Take {
    fn default() -> Self {
        Self {
            zeilen: Rows::default(),
            mengen: true,
            preise: true,
            markierung: None,
        }
    }
}

/// [`Command::Einfuegen`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insert {
    /// proAVA clipboard text (`proAVA-Zwischenablage|1|zeilen|…`), e.g. from
    /// [`Command::Kopieren`] in another window.
    pub daten: String,
    /// Insert after this row (OZ or `#id`) instead of at the cursor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nach: Option<String>,
    /// LV of `nach` (number or place); None = the LV the window shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lv: Option<String>,
    /// Where the rows come from (status bar: „KI fügt 4 Positionen aus … ein“).
    #[serde(default)]
    pub herkunft: String,
}

/// [`Command::Uebernehmen`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adopt {
    /// File with the changed project in the exchange folder.
    pub datei: String,
    /// [`StateReply::stand`] of the state the change was made on: when the
    /// project in the window changed since, the window refuses.
    pub stand: u64,
    /// The MCP tool („zeile_aendern“), for the line „KI: …“.
    #[serde(default)]
    pub werkzeug: String,
}

/// [`Command::ProjektOeffnen`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenProject {
    pub pfad: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub schreibgeschuetzt: bool,
}

/// [`Command::Suchen`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Search {
    #[serde(default)]
    pub suche: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kg: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub einheit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lv: Option<String>,
    /// At most so many hits in the answer (all are marked); None = 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_treffer: Option<usize>,
}

/// [`Command::ZiehenUeber`]: where the pointer is, what is dragged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DragOver {
    /// One drag gets one number (the end and the drop name it).
    pub kennung: u64,
    /// The pointer on the desktop in physical pixels.
    pub punkt_px: [f32; 2],
    /// „3 Zeilen“, „Titel 02 Lüftung mit 14 Positionen“.
    pub zusammenfassung: String,
    /// Where they come from: the project of the other window.
    #[serde(default)]
    pub herkunft: String,
}

/// [`Command::Ablegen`]: the rows let go at `punkt_px`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DragDrop {
    pub kennung: u64,
    pub punkt_px: [f32; 2],
    /// proAVA clipboard text of the rows (as [`Insert::daten`]).
    pub daten: String,
    #[serde(default)]
    pub herkunft: String,
}

/// The answer to [`Command::ZiehenUeber`]: what letting go would do here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DropTarget {
    /// „hinter 01.482.0010 einfügen“, „schreibgeschützt – hier geht nichts“.
    pub text: String,
    /// Letting go here inserts the rows.
    pub moeglich: bool,
}

// ----- Antworten -----

/// The answer of a window to one [`Request`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub v: u32,
    /// The `id` of the request.
    pub id: u64,
    pub antwort: Reply,
}

impl Response {
    pub fn new(id: u64, reply: Reply) -> Self {
        Self {
            v: VERSION,
            id,
            antwort: reply,
        }
    }
}

/// What the window says.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum Reply {
    Fenster(WindowInfo),
    /// Rows (lesen, zeigen, suchen, markieren).
    Zeilen(RowsReply),
    /// Rows as proAVA clipboard text (kopieren).
    Kopie(ClipReply),
    /// Done (einfuegen, projekt_oeffnen); `wartet` = a question of proAVA
    /// waits for the user.
    Erledigt(Done),
    /// Where a drag over this window would land (ziehen_ueber, ziehen_ende).
    Ziel(DropTarget),
    /// The whole project of the window (stand).
    Stand(StateReply),
    /// Not done, with the reason (read-only, locked, dialog open, unknown OZ …).
    Abgelehnt {
        text: String,
    },
    /// The window does not let assistants work in it (Extras ▸ Einstellungen).
    Aus {
        text: String,
    },
}

impl Reply {
    pub fn refused(text: impl Into<String>) -> Self {
        Reply::Abgelehnt { text: text.into() }
    }

    /// The text of a refusal or of the switched-off bridge.
    pub fn refusal(&self) -> Option<&str> {
        match self {
            Reply::Abgelehnt { text } | Reply::Aus { text } => Some(text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RowsReply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lv: Option<LvInfo>,
    pub zeilen: Vec<RowInfo>,
    /// Hits/rows in all (the list may be shortened).
    #[serde(default)]
    pub gesamt: usize,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClipReply {
    /// „3 Positionen“, „Titel 02 Lüftung mit 14 Positionen“.
    pub zusammenfassung: String,
    /// proAVA clipboard text for [`Insert::daten`].
    pub daten: String,
    pub lv: LvInfo,
    pub zeilen: Vec<RowInfo>,
    /// Project the rows come from (name).
    #[serde(default)]
    pub projekt: String,
}

/// [`Reply::Stand`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateReply {
    /// File in the exchange folder with the whole project as the window has
    /// it; the reader removes it.
    pub datei: String,
    /// Mark of this state; [`Adopt::stand`] names it.
    pub stand: u64,
    /// The project file of the window.
    pub projekt: String,
    #[serde(default)]
    pub ungespeichert: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Done {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zeilen: Vec<RowInfo>,
    /// A question of proAVA (e.g. after sending the LV) waits for the user.
    #[serde(default, skip_serializing_if = "is_false")]
    pub wartet: bool,
}

/// Place and size of a window on the screen (points of the desktop).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
    pub x: f32,
    pub y: f32,
    pub breite: f32,
    pub hoehe: f32,
}

impl Geometry {
    pub fn center_x(&self) -> f32 {
        self.x + self.breite / 2.0
    }

    /// Whether the point (same units) lies inside.
    pub fn contains(&self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] < self.x + self.breite
            && point[1] >= self.y
            && point[1] < self.y + self.hoehe
    }
}

/// The window under a point of the desktop (physical pixels) among
/// `windows`, leaving out `own_pid`: the one whose inside contains it; of
/// several lying on top of each other the one used last (focus).
pub fn window_at(windows: &[WindowEntry], own_pid: u32, point: [f32; 2]) -> Option<&WindowEntry> {
    windows
        .iter()
        .filter(|entry| entry.pid != own_pid && !entry.fenster.minimiert)
        .filter(|entry| {
            entry
                .fenster
                .innen_px
                .is_some_and(|inside| inside.contains(point))
        })
        .max_by_key(|entry| (entry.fenster.fokus, entry.fenster.fokus_zuletzt_ms))
}

/// The state of one window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub pid: u32,
    /// Program version („0.2.0“).
    #[serde(default)]
    pub programm: String,
    /// Assistants may work in this window (Extras ▸ Einstellungen).
    #[serde(default)]
    pub ki_erlaubt: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lage: Option<Geometry>,
    /// The inside of the window (without frame and title) on the desktop in
    /// physical pixels – where a drag from another window lands. None when
    /// the system does not tell (Wayland).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub innen_px: Option<Geometry>,
    /// Size of the screen the window is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bildschirm: Option<[f32; 2]>,
    #[serde(default)]
    pub fokus: bool,
    /// When the window last had the keyboard focus (ms since 1970; 0 = never).
    #[serde(default)]
    pub fokus_zuletzt_ms: u64,
    #[serde(default)]
    pub minimiert: bool,
    /// None: the start page is shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projekt: Option<ProjectInfo>,
    /// What the assistant is doing right now (status bar „KI: …“).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ki_taetigkeit: Option<String>,
    /// A dialog of proAVA is open (its title) – it waits for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<String>,
}

impl WindowInfo {
    /// „Rathaus 3 (LV 1 Gebäudeautomation)“, „Startseite“.
    pub fn label(&self) -> String {
        match &self.projekt {
            Some(project) => format!("{} ({})", project.name, project.lv.label()),
            None => "Startseite (kein Projekt offen)".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    /// Path of the project file.
    pub datei: String,
    /// The LV the window shows.
    pub lv: LvInfo,
    pub lvs: Vec<LvInfo>,
    pub ungespeichert: bool,
    pub schreibgeschuetzt: bool,
    /// Reiter in front: „LV“, „Angebote“ …
    pub reiter: String,
    /// The selected rows (the current one first).
    pub auswahl: Vec<RowInfo>,
    /// The X83 of this LV went to the bidders (changes of OZ ask first).
    #[serde(default)]
    pub versandt: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LvInfo {
    /// Place in the LV list, 1 = first.
    pub stelle: usize,
    pub nummer: String,
    pub titel: String,
    #[serde(default)]
    pub positionen: usize,
}

impl LvInfo {
    /// „LV 2 Elektro“, „LV Gebäudeautomation“.
    pub fn label(&self) -> String {
        let number = self.nummer.trim();
        let title = self.titel.trim();
        match (number.is_empty(), title.is_empty()) {
            (true, true) => format!("LV {}", self.stelle),
            (true, false) => format!("LV {title}"),
            (false, true) => format!("LV {number}"),
            (false, false) => format!("LV {number} {title}"),
        }
    }
}

/// One row of an LV as the assistant sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowInfo {
    pub id: u64,
    /// OZ (titles, positions), „A0001“, empty (hints, spacers).
    pub oz: String,
    /// „Titel“, „Position“, „Hinweis“, „Ausführungsbeschreibung“, „Abstand“.
    pub art: String,
    pub kurztext: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub menge: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub einheit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ep: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gp: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kg: String,
    /// Titles above: „01 Raumautomation › 01.02 Bediengeräte“.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub titelpfad: String,
    /// „fertig“ (gesperrt), „in Bearbeitung“, empty = offen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Search hits: a piece of text around the find.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub fundstelle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub langtext: Option<String>,
}

impl RowInfo {
    /// „01.02.0010 Position Raumbediengerät – 12 St · EP 85,00“, „Hinweis „Vorbemerkung“ (#12)“.
    pub fn line(&self) -> String {
        let mut line = if self.oz.is_empty() {
            format!("{} „{}“ (#{})", self.art, self.kurztext, self.id)
        } else {
            format!("{} {} {} (#{})", self.oz, self.art, self.kurztext, self.id)
        };
        if !self.menge.is_empty() || !self.einheit.is_empty() {
            line.push_str(&format!(" – {} {}", self.menge, self.einheit));
            line = line.trim_end().to_string();
        }
        if !self.ep.is_empty() {
            line.push_str(&format!(" · EP {}", self.ep));
        }
        if !self.kg.is_empty() {
            line.push_str(&format!(" · KG {}", self.kg));
        }
        if !self.status.is_empty() {
            line.push_str(&format!(" · {}", self.status));
        }
        line
    }
}

// ----- Verzeichnis -----

/// What a window writes into the folder (`<pid>.json`): how to reach it and
/// its state at the last change (the MCP server asks the window itself for
/// the current one and falls back to this).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowEntry {
    /// Protocol version of the window.
    pub v: u32,
    pub pid: u32,
    /// Unix socket (path) or named pipe (`\\.\pipe\…`).
    pub kanal: String,
    /// Start of the window (ms since 1970).
    #[serde(default)]
    pub gestartet_ms: u64,
    /// Time of this entry (ms since 1970).
    #[serde(default)]
    pub stand_ms: u64,
    pub fenster: WindowInfo,
}

// ----- Kodierung -----

/// One message as one line (without line break).
pub fn encode<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).unwrap_or_else(|_| "{}".to_string())
}

/// One line back into a message; empty lines and foreign text are errors.
pub fn decode<T: DeserializeOwned>(line: &str) -> Result<T, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("leere Zeile".to_string());
    }
    if line.len() > MAX_LINE {
        return Err(format!("Zeile zu lang ({} Bytes)", line.len()));
    }
    serde_json::from_str(line).map_err(|error| format!("unverständliche Nachricht: {error}"))
}

/// A request line as the window reads it: the request, or the response to
/// send right away (unreadable line, other protocol version).
pub fn read_request(line: &str) -> Result<Request, Box<Response>> {
    let value: serde_json::Value =
        decode(line).map_err(|error| Box::new(Response::new(0, Reply::refused(error))))?;
    let id = value
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let version = value.get("v").and_then(serde_json::Value::as_u64);
    if version != Some(u64::from(VERSION)) {
        return Err(Box::new(Response::new(
            id,
            Reply::refused(format!(
                "Protokoll {} wird nicht verstanden – dieses proAVA spricht Version {VERSION}. \
                 proAVA und proava-mcp auf denselben Stand bringen.",
                version.map_or("ohne Version".to_string(), |v| format!("Version {v}"))
            )),
        )));
    }
    serde_json::from_value(value).map_err(|error| {
        Box::new(Response::new(
            id,
            Reply::refused(format!("Anfrage nicht verstanden: {error}")),
        ))
    })
}

// ----- Fenster wählen -----

/// Names of a window among the open ones: „links“/„rechts“ from the place on
/// the screen (middle of the window), „aktiv“ for the one that had the focus
/// last. One window alone is all of them.
pub fn names(windows: &[WindowInfo]) -> Vec<Vec<&'static str>> {
    let mut names = vec![Vec::new(); windows.len()];
    if windows.is_empty() {
        return names;
    }
    let placed: Vec<(usize, f32)> = windows
        .iter()
        .enumerate()
        .filter_map(|(index, window)| window.lage.map(|place| (index, place.center_x())))
        .collect();
    if windows.len() == 1 {
        names[0] = vec!["links", "rechts", "aktiv"];
        return names;
    }
    if placed.len() >= 2 {
        let left = placed
            .iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|entry| entry.0);
        let right = placed
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|entry| entry.0);
        if let (Some(left), Some(right)) = (left, right) {
            if left != right {
                names[left].push("links");
                names[right].push("rechts");
            }
        }
    }
    if let Some(active) = active(windows) {
        names[active].push("aktiv");
    }
    names
}

/// The window with the focus, else the one that had it last.
pub fn active(windows: &[WindowInfo]) -> Option<usize> {
    windows
        .iter()
        .position(|window| window.fokus)
        .or_else(|| {
            windows
                .iter()
                .enumerate()
                .filter(|(_, window)| window.fokus_zuletzt_ms > 0)
                .max_by_key(|(_, window)| window.fokus_zuletzt_ms)
                .map(|(index, _)| index)
        })
        .or((windows.len() == 1).then_some(0))
}

/// The window meant by `wanted`: „links“, „rechts“, „aktiv“, a PID, or a part
/// of the project name or file name (case and umlauts as written).
pub fn pick(windows: &[WindowInfo], wanted: &str) -> Result<usize, String> {
    let wanted = wanted.trim();
    if windows.is_empty() {
        return Err(
            "Kein proAVA-Fenster offen (oder keines erlaubt KI-Assistenten den Zugriff)."
                .to_string(),
        );
    }
    let key = wanted.to_lowercase();
    let key = match key.as_str() {
        "" => "aktiv".to_string(),
        "link" | "linke" | "linkes" => "links".to_string(),
        "recht" | "rechte" | "rechtes" => "rechts".to_string(),
        "aktive" | "aktives" | "vorne" => "aktiv".to_string(),
        _ => key,
    };
    if matches!(key.as_str(), "links" | "rechts" | "aktiv") {
        let names = names(windows);
        if let Some(index) = names.iter().position(|names| names.contains(&key.as_str())) {
            return Ok(index);
        }
        return Err(match key.as_str() {
            "aktiv" => "Kein Fenster hatte bisher den Fokus – bitte links, rechts, den Projektnamen oder die PID nennen.".to_string(),
            side => format!(
                "Welches Fenster {side} liegt, ist nicht bekannt (die Lage meldet das System nicht) – bitte den Projektnamen oder die PID nennen: {}.",
                listing(windows)
            ),
        });
    }
    if let Ok(pid) = key.parse::<u32>() {
        return windows
            .iter()
            .position(|window| window.pid == pid)
            .ok_or_else(|| format!("Kein proAVA-Fenster mit PID {pid}: {}.", listing(windows)));
    }
    let matching: Vec<usize> = windows
        .iter()
        .enumerate()
        .filter(|(_, window)| {
            window.projekt.as_ref().is_some_and(|project| {
                project.name.to_lowercase().contains(&key)
                    || file_stem(&project.datei).to_lowercase().contains(&key)
            })
        })
        .map(|(index, _)| index)
        .collect();
    match matching.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!(
            "Kein proAVA-Fenster mit „{wanted}“: {}.",
            listing(windows)
        )),
        _ => Err(format!(
            "„{wanted}“ passt auf mehrere Fenster – bitte links, rechts oder die PID nennen: {}.",
            matching
                .iter()
                .map(|&index| short(&windows[index]))
                .collect::<Vec<_>>()
                .join("; ")
        )),
    }
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn short(window: &WindowInfo) -> String {
    format!("PID {} {}", window.pid, window.label())
}

fn listing(windows: &[WindowInfo]) -> String {
    let names = names(windows);
    windows
        .iter()
        .zip(names)
        .map(|(window, names)| {
            if names.is_empty() {
                short(window)
            } else {
                format!("{} [{}]", short(window), names.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(pid: u32, name: &str, x: Option<f32>, focus_ms: u64) -> WindowInfo {
        WindowInfo {
            pid,
            ki_erlaubt: true,
            lage: x.map(|x| Geometry {
                x,
                y: 0.0,
                breite: 900.0,
                hoehe: 700.0,
            }),
            fokus_zuletzt_ms: focus_ms,
            projekt: (!name.is_empty()).then(|| ProjectInfo {
                name: name.to_string(),
                datei: format!("/projekte/{name}/{name}.proava"),
                ..ProjectInfo::default()
            }),
            ..WindowInfo::default()
        }
    }

    #[test]
    fn file_names_and_pids() {
        assert_eq!(entry_file_name(4711), "4711.json");
        assert_eq!(socket_file_name(4711), "4711.sock");
        assert_eq!(pid_of_entry("4711.json"), Some(4711));
        assert_eq!(pid_of_socket("4711.sock"), Some(4711));
        assert_eq!(pid_of_entry("4711.sock"), None);
        assert_eq!(pid_of_entry(".json"), None);
        assert_eq!(pid_of_entry("+1.json"), None);
        assert_eq!(pid_of_entry("12a.json"), None);
        assert_eq!(pid_of_entry("99999999999.json"), None, "keine gültige PID");
    }

    #[test]
    fn requests_and_responses_go_through_one_line() {
        let requests = [
            Request::new(1, Command::FensterLesen),
            Request::new(
                2,
                Command::Zeigen(Show {
                    zeilen: Rows {
                        oz: vec!["01.02.0010".into()],
                        weitere: 3,
                        ..Rows::default()
                    },
                    positionsfenster: true,
                    markierung: Some("Schaltschrank".into()),
                }),
            ),
            Request::new(
                3,
                Command::Einfuegen(Insert {
                    daten: "proAVA-Zwischenablage|1|zeilen|1 Position|UEsDB…".into(),
                    nach: Some("#12".into()),
                    lv: None,
                    herkunft: "Rathaus 3".into(),
                }),
            ),
            Request::new(4, Command::Kopieren(Take::default())),
            Request::new(
                5,
                Command::Suchen(Search {
                    suche: "Schaltschrank".into(),
                    ..Search::default()
                }),
            ),
        ];
        for request in requests {
            let line = encode(&request);
            assert!(!line.contains('\n'), "eine Zeile: {line}");
            assert_eq!(read_request(&line).unwrap(), request);
        }
        let response = Response::new(
            7,
            Reply::Erledigt(Done {
                text: "4 Positionen eingefügt.".into(),
                zeilen: vec![RowInfo {
                    id: 12,
                    oz: "01.0010".into(),
                    art: "Position".into(),
                    kurztext: "Schaltschrank\nzweite Zeile".into(),
                    ..RowInfo::default()
                }],
                wartet: false,
            }),
        );
        let line = encode(&response);
        assert!(!line.contains('\n'));
        assert_eq!(decode::<Response>(&line).unwrap(), response);
    }

    #[test]
    fn stand_and_takeover_go_both_ways() {
        let request = read_request(r#"{"v":1,"id":3,"befehl":{"art":"stand"}}"#).unwrap();
        assert_eq!(request.befehl, Command::Stand);
        assert!(!request.befehl.changes_project());
        let adopt = Request::new(
            4,
            Command::Uebernehmen(Adopt {
                datei: "/tmp/x/austausch/mcp-1.proava".into(),
                stand: 12,
                werkzeug: "zeile_aendern".into(),
            }),
        );
        let line = encode(&adopt);
        let back = read_request(&line).unwrap();
        assert_eq!(back, adopt);
        assert!(back.befehl.changes_project());
        assert_eq!(back.befehl.name(), "uebernehmen");
        let save = read_request(r#"{"v":1,"id":5,"befehl":{"art":"speichern"}}"#).unwrap();
        assert_eq!(save.befehl, Command::Speichern);
        let reply = Response::new(
            3,
            Reply::Stand(StateReply {
                datei: "/tmp/x/austausch/4711-3.proava".into(),
                stand: 12,
                projekt: "/p/Rathaus.proava".into(),
                ungespeichert: true,
            }),
        );
        assert_eq!(decode::<Response>(&encode(&reply)).unwrap(), reply);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let request =
            read_request(r#"{"v":1,"id":9,"befehl":{"art":"kopieren","zeilen":{"auswahl":true}}}"#)
                .unwrap();
        assert_eq!(
            request.befehl,
            Command::Kopieren(Take {
                zeilen: Rows::selection(),
                mengen: true,
                preise: true,
                markierung: None
            })
        );
        assert_eq!(request.befehl.name(), "kopieren");
        assert!(!request.befehl.changes_project());
    }

    #[test]
    fn other_versions_and_foreign_text_are_answered() {
        let answer =
            read_request(r#"{"v":2,"id":5,"befehl":{"art":"fenster_lesen"}}"#).unwrap_err();
        assert_eq!(answer.id, 5);
        let text = answer.antwort.refusal().unwrap();
        assert!(
            text.contains("Version 2") && text.contains("Version 1"),
            "{text}"
        );
        let answer = read_request(r#"{"id":6,"befehl":{"art":"fenster_lesen"}}"#).unwrap_err();
        assert!(answer.antwort.refusal().unwrap().contains("ohne Version"));
        let answer = read_request("hallo").unwrap_err();
        assert_eq!(answer.id, 0);
        assert!(answer.antwort.refusal().unwrap().contains("unverständlich"));
        let answer = read_request(r#"{"v":1,"id":8,"befehl":{"art":"loeschen"}}"#).unwrap_err();
        assert_eq!(answer.id, 8);
        assert!(answer
            .antwort
            .refusal()
            .unwrap()
            .contains("nicht verstanden"));
        assert!(decode::<Response>("   ").is_err());
    }

    #[test]
    fn left_right_and_active_from_the_screen() {
        let windows = vec![
            window(30, "Rathaus 3", Some(960.0), 2_000),
            window(20, "Schule Musterstadt", Some(0.0), 5_000),
        ];
        let names = names(&windows);
        assert_eq!(names[0], vec!["rechts"]);
        assert_eq!(names[1], vec!["links", "aktiv"]);
        assert_eq!(pick(&windows, "links"), Ok(1));
        assert_eq!(pick(&windows, "Rechts"), Ok(0));
        assert_eq!(pick(&windows, "rechte"), Ok(0));
        assert_eq!(pick(&windows, "aktiv"), Ok(1));
        assert_eq!(pick(&windows, ""), Ok(1), "ohne Angabe das aktive");
        assert_eq!(pick(&windows, "30"), Ok(0));
        assert_eq!(pick(&windows, "rathaus"), Ok(0));
        assert_eq!(pick(&windows, "Musterstadt"), Ok(1));
        let error = pick(&windows, "Bahnhof").unwrap_err();
        assert!(
            error.contains("PID 30 Rathaus 3") && error.contains("[links, aktiv]"),
            "{error}"
        );
        assert!(pick(&windows, "99").unwrap_err().contains("PID 99"));
        assert!(pick(&[], "links")
            .unwrap_err()
            .contains("Kein proAVA-Fenster"));
    }

    #[test]
    fn the_focused_window_is_active_and_one_window_is_everything() {
        let mut windows = vec![
            window(1, "A", Some(0.0), 9_000),
            window(2, "B", Some(1000.0), 1_000),
        ];
        windows[1].fokus = true;
        assert_eq!(active(&windows), Some(1));
        let one = vec![window(7, "", None, 0)];
        assert_eq!(names(&one)[0], vec!["links", "rechts", "aktiv"]);
        assert_eq!(pick(&one, "rechts"), Ok(0));
        assert_eq!(one[0].label(), "Startseite (kein Projekt offen)");
    }

    #[test]
    fn without_places_left_and_right_are_unknown() {
        let windows = vec![window(1, "A", None, 0), window(2, "A Kopie", None, 0)];
        let error = pick(&windows, "links").unwrap_err();
        assert!(error.contains("nicht bekannt"), "{error}");
        assert!(pick(&windows, "aktiv").unwrap_err().contains("Fokus"));
        let error = pick(&windows, "a").unwrap_err();
        assert!(error.contains("mehrere"), "{error}");
        assert_eq!(pick(&windows, "kopie"), Ok(1));
    }

    #[test]
    fn three_windows_have_a_middle_without_name() {
        let windows = vec![
            window(1, "A", Some(0.0), 0),
            window(2, "B", Some(700.0), 0),
            window(3, "C", Some(1400.0), 0),
        ];
        let names = names(&windows);
        assert_eq!(names[0], vec!["links"]);
        assert!(names[1].is_empty());
        assert_eq!(names[2], vec!["rechts"]);
    }

    #[test]
    fn the_window_under_the_pointer_is_found_on_the_desktop() {
        let inside = |x: f32| Geometry {
            x,
            y: 30.0,
            breite: 900.0,
            hoehe: 700.0,
        };
        let entry = |pid: u32, x: Option<f32>, focus: u64| WindowEntry {
            pid,
            fenster: WindowInfo {
                pid,
                innen_px: x.map(inside),
                fokus_zuletzt_ms: focus,
                ..WindowInfo::default()
            },
            ..WindowEntry::default()
        };
        let windows = vec![
            entry(1, Some(0.0), 10),
            entry(2, Some(960.0), 5),
            entry(3, Some(1000.0), 20),
            entry(4, None, 99),
        ];
        assert_eq!(
            window_at(&windows, 1, [1200.0, 300.0]).map(|e| e.pid),
            Some(3),
            "zuletzt aktiv"
        );
        assert_eq!(
            window_at(&windows, 1, [970.0, 300.0]).map(|e| e.pid),
            Some(2)
        );
        assert_eq!(
            window_at(&windows, 1, [500.0, 300.0]),
            None,
            "das eigene Fenster zählt nicht"
        );
        assert_eq!(
            window_at(&windows, 2, [500.0, 300.0]).map(|e| e.pid),
            Some(1)
        );
        assert_eq!(
            window_at(&windows, 1, [1200.0, 10.0]),
            None,
            "Titelleiste ist nicht innen"
        );
        assert!(Command::Ablegen(DragDrop::default()).is_drag());
        assert!(Command::ZiehenEnde { kennung: 1 }.is_drag());
        assert!(!Command::Einfuegen(Insert::default()).is_drag());
        let request = Request::new(
            3,
            Command::ZiehenUeber(DragOver {
                kennung: 7,
                punkt_px: [1200.5, 300.0],
                zusammenfassung: "3 Zeilen".into(),
                herkunft: "Schule Musterstadt".into(),
            }),
        );
        assert_eq!(read_request(&encode(&request)).unwrap(), request);
    }

    #[test]
    fn row_lines_for_answers() {
        let row = RowInfo {
            id: 12,
            oz: "01.02.0010".into(),
            art: "Position".into(),
            kurztext: "Raumbediengerät".into(),
            menge: "12".into(),
            einheit: "St".into(),
            ep: "85,00".into(),
            kg: "481".into(),
            status: "fertig".into(),
            ..RowInfo::default()
        };
        assert_eq!(
            row.line(),
            "01.02.0010 Position Raumbediengerät (#12) – 12 St · EP 85,00 · KG 481 · fertig"
        );
        let note = RowInfo {
            id: 3,
            art: "Hinweis".into(),
            kurztext: "Vorbemerkung".into(),
            ..RowInfo::default()
        };
        assert_eq!(note.line(), "Hinweis „Vorbemerkung“ (#3)");
        assert_eq!(
            LvInfo {
                stelle: 2,
                nummer: "2".into(),
                titel: "Elektro".into(),
                positionen: 0
            }
            .label(),
            "LV 2 Elektro"
        );
    }
}
