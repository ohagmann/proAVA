use std::path::{Path, PathBuf};

use proava_domain::{Address, Decimal, DeviationReference, OzMask, DEFAULT_POSITION_STEP};

/// The two appearances the program offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn flipped(self) -> Self {
        match self {
            ThemeChoice::Light => ThemeChoice::Dark,
            ThemeChoice::Dark => ThemeChoice::Light,
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "light" => Some(ThemeChoice::Light),
            "dark" => Some(ThemeChoice::Dark),
            _ => None,
        }
    }
}

/// Where the workspace shows the edit panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelSide {
    #[default]
    Left,
    Right,
}

impl PanelSide {
    pub fn flipped(self) -> Self {
        match self {
            PanelSide::Left => PanelSide::Right,
            PanelSide::Right => PanelSide::Left,
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            PanelSide::Left => "left",
            PanelSide::Right => "right",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "left" => Some(PanelSide::Left),
            "right" => Some(PanelSide::Right),
            _ => None,
        }
    }
}

/// What kind of file a recent entry points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    ProAva,
    /// Any other file (no proAVA project file).
    Other,
}

impl ProjectKind {
    pub fn from_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if extension == "proava" {
            ProjectKind::ProAva
        } else {
            ProjectKind::Other
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProjectKind::ProAva => "proAVA",
            ProjectKind::Other => "andere Datei",
        }
    }

    pub fn as_key(self) -> &'static str {
        match self {
            ProjectKind::ProAva => "proava",
            ProjectKind::Other => "other",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "proava" => Some(ProjectKind::ProAva),
            "other" => Some(ProjectKind::Other),
            _ => None,
        }
    }
}

/// One line in the "zuletzt verwendet" list.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentProject {
    pub name: String,
    pub path: PathBuf,
    pub opened_at: String,
    pub kind: ProjectKind,
}

/// Size (width, height) and screen position (left, top) of a window of its
/// own, in points. The position may be unknown (Wayland tells none).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WindowPlace {
    pub size: Option<(f32, f32)>,
    pub at: Option<(f32, f32)>,
}

/// Everything the program remembers between starts.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub theme: Option<ThemeChoice>,
    pub recent: Vec<RecentProject>,
    pub last_dir: Option<PathBuf>,
    /// Folder offered first when a new project is created.
    pub default_folder: Option<PathBuf>,
    /// Side and last width of the workspace's edit panel.
    pub detail_side: PanelSide,
    pub detail_width: Option<f32>,
    /// Widths of the LV table columns the user dragged ("oz", "qty", "unit", "ep", "gp").
    pub column_widths: std::collections::BTreeMap<String, f32>,
    /// Optional columns of the LV table switched on or off ("kg", "lb");
    /// missing = the column's default (KG on, LB off).
    pub table_columns: std::collections::BTreeMap<String, bool>,
    /// Print dialog options per kind (key of `PrintKind` → `PrintSettings::to_line`).
    pub print: std::collections::BTreeMap<String, String>,
    /// Size of the own window for long texts (width, height in points).
    pub long_text_window: Option<(f32, f32)>,
    /// Size and screen position of the window „Eigenschaften“ of a row.
    pub position_window: WindowPlace,
    /// Everything the settings dialog offers (Einstellungen).
    pub preferences: Preferences,
    /// Sources of the last takeovers (projects, GAEB files), newest first.
    pub transfer_sources: Vec<PathBuf>,
    /// Folders last used in a file dialog, newest first (Zuletzt benutzt).
    pub file_dialog_folders: Vec<PathBuf>,
    /// Personal spelling dictionary („Zum Wörterbuch hinzufügen“).
    pub personal_words: Vec<String>,
    /// The side window „Bibliothek“ in the LV is shown.
    pub library_panel: bool,
    /// Template last chosen for a new project (its name; None = „leer“).
    pub last_template: Option<String>,
    /// What loading had to repair (damaged file) – shown once, not stored.
    pub notes: Vec<String>,
}

/// Settings the user chooses in Extras ▸ Einstellungen. Every field has a
/// sensible default so older settings files keep working.
#[derive(Debug, Clone, PartialEq)]
pub struct Preferences {
    // Darstellung
    /// Zoom of the whole interface, 1.0 = 100 %.
    pub ui_zoom: f32,
    /// Whether the workspace shows the edit panel (Eigenschaftenfenster).
    pub detail_visible: bool,
    /// Mark positions without quantity in the LV (read by the status markers).
    pub mark_empty_quantities: bool,
    /// Vorschau aller PDF-Ausschnitte im Langtext ausblenden (only placeholders).
    pub clip_previews_hidden: bool,
    /// The windows „Eigenschaften“ and „Langtext“ belong to the main window
    /// (owner/transient window): in front of it, not of other programs.
    pub windows_in_front: bool,
    /// Title rows of the tables in yellow (Titel Hellgelb 3, Untertitel
    /// Hellgelb 4; dark: muted yellow). Default on.
    pub title_rows_filled: bool,
    // Neue Projekte
    /// OZ-Maske of new LVs; a file without the entry gets
    /// [`OzMask::NEW_LV_PATTERN`], a stored one stays.
    pub new_lv_mask: String,
    pub new_lv_step: u32,
    pub new_lv_vat: Decimal,
    // Speichern
    pub autosave: bool,
    pub autosave_minutes: u32,
    /// Write a backup (Zip) when a project is closed.
    pub backup_on_close: bool,
    // Preisspiegel
    pub outlier_percent: u32,
    pub deviation_reference: DeviationReference,
    pub undercut_percent: u32,
    // Drucken
    /// Planner (Aufsteller) filled in for new projects.
    pub default_planner: Address,
    /// Image printed at the top right of the title block.
    pub logo_file: Option<PathBuf>,
    /// The priced LV gets the cover sheet too (the request for quotation always does).
    pub cover_on_priced_lv: bool,
    /// Kopf- und Fußzeile of all printouts; a project may have its own.
    pub page_frame: proava_domain::PageFrame,
    /// Blocks of the Deckblatt (logo, addresses, facts, bidder field).
    pub cover_blocks: crate::CoverBlocks,
    /// Druckstil (default for all kinds) and accent colour of the printouts.
    pub print_look: crate::PrintLook,
    /// Bearbeiter (a person) for `{ersteller}`/`{bearbeiter}`; empty = the planner.
    pub print_author: String,
    // GAEB-Export
    /// Bilder der Langtexte in GAEB DA XML mitgeben (Exportdialog, Vorgabe an).
    pub gaeb_images: bool,
    /// CSV-Export in Windows-1252 statt UTF-8 mit BOM (Exportdialog, gemerkt).
    pub csv_windows_1252: bool,
    /// AUSSCHREIBEN.DE and STLB-Bau Online in the system browser instead of
    /// proAVA's own browser window (helper `proava-web`; default: own
    /// window when it is installed).
    pub web_pages_in_system_browser: bool,
    /// KI-Assistenten (MCP-Server) dürfen in geöffneten Fenstern arbeiten:
    /// zeigen, markieren, suchen, einfügen (lokale Brücke; Vorgabe an). Aus:
    /// die Brücke antwortet nur mit einem Hinweis.
    pub ai_windows: bool,
    /// Own position library (a proAVA project); None = the default file
    /// in the home folder.
    pub library_file: Option<PathBuf>,
    /// Further libraries (Büro, Netzlaufwerk …), usually read-only.
    pub libraries: Vec<LibrarySetting>,
    /// „Bibliothek bearbeiten …“ opens the library in a new proAVA window
    /// (the project stays open); off = in this window, with „Zurück zu …“.
    pub library_own_window: bool,
    /// Folder of the own project templates; None = `proAVA-Vorlagen` in
    /// the home folder (next to the library).
    pub template_folder: Option<PathBuf>,
    // Rechtschreibung
    /// Red wavy line under unknown words in texts.
    pub spell_check: bool,
    /// Folder with de_DE.aff/.dic; None = search the system.
    pub dictionary_folder: Option<PathBuf>,
    // Textergänzungen
    /// Vortexte of the quick choice „Bieterangabe einfügen“ in the long text.
    pub bidder_captions: Vec<String>,
    /// Vortexte of the quick choice „Angabe des Auftraggebers einfügen“.
    pub owner_captions: Vec<String>,
}

/// Quick choice for Bieterangaben in the long text – what a GA tender
/// usually asks the bidder for.
pub fn default_bidder_captions() -> Vec<String> {
    [
        "Fabrikat:",
        "Typ:",
        "Hersteller:",
        "Erzeugnis:",
        "Angebotenes Produkt:",
        "Fabrikat/Typ:",
        "Software/Version:",
    ]
    .map(str::to_string)
    .to_vec()
}

/// Quick choice for Angaben des Auftraggebers (planner's values).
pub fn default_owner_captions() -> Vec<String> {
    [
        "Zahlenwert:",
        "Ausprägung:",
        "Anzahl:",
        "Nennweite:",
        "Leistung:",
        "Abmessungen:",
    ]
    .map(str::to_string)
    .to_vec()
}

/// A list typed one entry per line: trimmed, empty lines and doubles gone.
pub fn caption_list(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.is_empty() && !out.iter().any(|known| known == line) {
            out.push(line.to_string());
        }
    }
    out
}

/// One further library in Extras ▸ Einstellungen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LibrarySetting {
    pub path: PathBuf,
    /// Name in the list, e.g. „Büro“; empty = the file name.
    pub label: String,
    /// Only read (nothing is stored there).
    pub read_only: bool,
}

pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.7..=2.0;
pub const AUTOSAVE_MINUTES_RANGE: std::ops::RangeInclusive<u32> = 1..=120;

impl Default for Preferences {
    fn default() -> Self {
        Self {
            ui_zoom: 1.0,
            detail_visible: true,
            mark_empty_quantities: true,
            clip_previews_hidden: false,
            windows_in_front: true,
            title_rows_filled: true,
            new_lv_mask: OzMask::NEW_LV_PATTERN.to_string(),
            new_lv_step: DEFAULT_POSITION_STEP,
            new_lv_vat: Decimal::from(19),
            autosave: true,
            autosave_minutes: 5,
            backup_on_close: false,
            outlier_percent: 20,
            deviation_reference: DeviationReference::Mean,
            undercut_percent: 10,
            default_planner: Address::default(),
            logo_file: None,
            cover_on_priced_lv: false,
            page_frame: proava_domain::PageFrame::default(),
            cover_blocks: crate::CoverBlocks::default(),
            print_look: crate::PrintLook::default(),
            print_author: String::new(),
            gaeb_images: true,
            csv_windows_1252: false,
            web_pages_in_system_browser: false,
            ai_windows: true,
            library_file: None,
            libraries: Vec::new(),
            library_own_window: true,
            template_folder: None,
            spell_check: true,
            dictionary_folder: None,
            bidder_captions: default_bidder_captions(),
            owner_captions: default_owner_captions(),
        }
    }
}

impl Preferences {
    /// Plain problems for the user; empty = the preferences can be saved.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !ZOOM_RANGE.contains(&self.ui_zoom) {
            problems.push("Der Zoom liegt zwischen 70 und 200 %.".to_string());
        }
        if let Err(problem) = OzMask::parse(&self.new_lv_mask) {
            problems.push(format!("OZ-Maske: {problem}"));
        }
        if ![1, 10, 100, 1000].contains(&self.new_lv_step) {
            problems.push("Die Schrittweite ist 1, 10, 100 oder 1000.".to_string());
        }
        if self.new_lv_vat < Decimal::ZERO || self.new_lv_vat > Decimal::ONE_HUNDRED {
            problems.push("Die Umsatzsteuer liegt zwischen 0 und 100 %.".to_string());
        }
        if !AUTOSAVE_MINUTES_RANGE.contains(&self.autosave_minutes) {
            problems.push(
                "Das Intervall für das automatische Sichern liegt zwischen 1 und 120 Minuten."
                    .to_string(),
            );
        }
        if !(1..=100).contains(&self.outlier_percent) {
            problems.push("Die Ausreißerschwelle liegt zwischen 1 und 100 %.".to_string());
        }
        if !(1..=50).contains(&self.undercut_percent) {
            problems.push("Die Aufgreifschwelle liegt zwischen 1 und 50 %.".to_string());
        }
        if self.bidder_captions.len() > 40 || self.owner_captions.len() > 40 {
            problems.push(
                "Die Schnellauswahl für Textergänzungen hat höchstens 40 Einträge.".to_string(),
            );
        }
        problems
    }
}

/// Keys for the deviation reference in the settings file.
pub fn reference_key(reference: DeviationReference) -> &'static str {
    match reference {
        DeviationReference::Mean => "mean",
        DeviationReference::Lowest => "lowest",
        DeviationReference::Estimate => "estimate",
    }
}

pub fn reference_from_key(key: &str) -> Option<DeviationReference> {
    match key {
        "mean" => Some(DeviationReference::Mean),
        "lowest" => Some(DeviationReference::Lowest),
        "estimate" => Some(DeviationReference::Estimate),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{reference_from_key, reference_key, Preferences};
    use proava_domain::{Decimal, DeviationReference};

    #[test]
    fn default_preferences_are_valid_and_problems_are_found() {
        let mut preferences = Preferences::default();
        assert!(preferences.problems().is_empty());
        preferences.ui_zoom = 3.0;
        preferences.new_lv_mask = "xx".into();
        preferences.new_lv_step = 7;
        preferences.new_lv_vat = Decimal::from(101);
        preferences.autosave_minutes = 0;
        preferences.outlier_percent = 0;
        preferences.undercut_percent = 90;
        assert_eq!(
            preferences.problems().len(),
            7,
            "{:?}",
            preferences.problems()
        );
        for reference in [
            DeviationReference::Mean,
            DeviationReference::Lowest,
            DeviationReference::Estimate,
        ] {
            assert_eq!(
                reference_from_key(reference_key(reference)),
                Some(reference)
            );
        }
    }
}
