use std::fs;
use std::path::PathBuf;

use proava_application::{
    reference_from_key, reference_key, LibrarySetting, PanelSide, Preferences, ProjectKind,
    RecentProject, Settings, SettingsFiles, ThemeChoice,
};
use proava_domain::{Address, Decimal};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Default, Serialize, Deserialize)]
struct SettingsDto {
    #[serde(default)]
    theme: Option<String>,
    #[serde(default)]
    recent: Vec<RecentDto>,
    #[serde(default)]
    last_dir: Option<String>,
    #[serde(default)]
    default_folder: Option<String>,
    #[serde(default)]
    detail_side: Option<String>,
    #[serde(default)]
    detail_width: Option<f32>,
    #[serde(default)]
    column_widths: std::collections::BTreeMap<String, f32>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    table_columns: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    print: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    long_text_window: Option<(f32, f32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position_window: Option<(f32, f32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position_window_at: Option<(f32, f32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    transfer_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    file_dialog_folders: Vec<String>,
    #[serde(default)]
    preferences: PreferencesDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    personal_words: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_template: Option<String>,
    #[serde(default)]
    library_panel: bool,
}

/// A further library (path, name, read-only).
#[derive(Debug, Default, Serialize, Deserialize)]
struct LibraryDto {
    path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    label: String,
    #[serde(default)]
    read_only: bool,
}

/// Extras ▸ Einstellungen; every field optional so old files keep loading.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PreferencesDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ui_zoom: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detail_visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mark_empty_quantities: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clip_previews_hidden: Option<bool>,
    /// Stored only when switched off (the default is on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    windows_in_front: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title_rows_filled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    new_lv_mask: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    new_lv_step: Option<u32>,
    /// Decimal as text, e.g. "19".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    new_lv_vat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    autosave: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    autosave_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backup_on_close: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outlier_percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deviation_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    undercut_percent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_planner: Option<AddressDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logo_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover_on_priced_lv: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    page_frame: Option<PageFrameDto>,
    /// Blocks of the Deckblatt: logo, adressen, kenndaten, bieterfeld.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover_blocks: Option<std::collections::BTreeMap<String, bool>>,
    /// Druckstil „klassisch“ or „proava“ (default for all kinds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_style: Option<String>,
    /// Accent colour of the printouts, „#RRGGBB“.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_accent: Option<String>,
    /// Blattrahmen by default and its width („duenn“, „mittel“).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_border: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_border_width: Option<String>,
    /// Tabellen voll umrandet (Gitter) by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_table_grid: Option<bool>,
    /// Zeilenabstand of the texts („einfach“, „1,15“, „1,5“), editor and print.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text_line_spacing: Option<String>,
    /// Abstand nach Absatz („0“, „3“, „6“, „halbe-zeile“), editor and print.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text_paragraph_gap: Option<String>,
    /// Bearbeiter for the printouts (person).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    print_author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gaeb_images: Option<bool>,
    /// CSV-Export in Windows-1252 (only written when on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    csv_windows_1252: Option<bool>,
    /// Text sources in the system browser instead of the own window (only written when on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    web_pages_in_system_browser: Option<bool>,
    /// KI-Assistenten in geöffneten Fenstern (only written when off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ai_windows: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    library_file: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    libraries: Vec<LibraryDto>,
    /// Bibliothek in einem eigenen Fenster bearbeiten (only written when off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    library_own_window: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    template_folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spell_check: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dictionary_folder: Option<String>,
    /// Quick choice of the Bieterangaben (only written when changed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bidder_captions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner_captions: Option<Vec<String>>,
}

/// Kopf-/Fußzeile; missing keys take the standard.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PageFrameDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    header: Option<[String; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    footer: Option<[String; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    header_rule: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    footer_rule: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    logo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    font_size_pt: Option<u8>,
}

impl PageFrameDto {
    fn into_frame(self) -> proava_domain::PageFrame {
        let default = proava_domain::PageFrame::default();
        proava_domain::PageFrame {
            header: self.header.unwrap_or(default.header),
            footer: self.footer.unwrap_or(default.footer),
            header_rule: self.header_rule.unwrap_or(default.header_rule),
            footer_rule: self.footer_rule.unwrap_or(default.footer_rule),
            logo: self
                .logo
                .as_deref()
                .and_then(proava_domain::FrameLogoPlace::from_key)
                .unwrap_or(default.logo),
            font_size_pt: self.font_size_pt.unwrap_or(default.font_size_pt),
        }
        .normalized()
    }

    fn of(frame: &proava_domain::PageFrame) -> Self {
        Self {
            header: Some(frame.header.clone()),
            footer: Some(frame.footer.clone()),
            header_rule: Some(frame.header_rule),
            footer_rule: Some(frame.footer_rule),
            logo: Some(frame.logo.key().to_string()),
            font_size_pt: Some(frame.font_size_pt),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AddressDto {
    #[serde(default)]
    name1: String,
    #[serde(default)]
    name2: String,
    #[serde(default)]
    name3: String,
    #[serde(default)]
    name4: String,
    #[serde(default)]
    street: String,
    #[serde(default)]
    postal_code: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    iln: String,
    #[serde(default)]
    contact: String,
    #[serde(default)]
    phone: String,
    #[serde(default)]
    fax: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    vat_id: String,
}

impl PreferencesDto {
    fn into_preferences(self) -> Preferences {
        let default = Preferences::default();
        let preferences = Preferences {
            ui_zoom: self.ui_zoom.unwrap_or(default.ui_zoom),
            detail_visible: self.detail_visible.unwrap_or(default.detail_visible),
            mark_empty_quantities: self
                .mark_empty_quantities
                .unwrap_or(default.mark_empty_quantities),
            clip_previews_hidden: self
                .clip_previews_hidden
                .unwrap_or(default.clip_previews_hidden),
            windows_in_front: self.windows_in_front.unwrap_or(default.windows_in_front),
            title_rows_filled: self.title_rows_filled.unwrap_or(default.title_rows_filled),
            new_lv_mask: self.new_lv_mask.unwrap_or(default.new_lv_mask.clone()),
            new_lv_step: self.new_lv_step.unwrap_or(default.new_lv_step),
            new_lv_vat: self
                .new_lv_vat
                .and_then(|text| text.parse::<Decimal>().ok())
                .unwrap_or(default.new_lv_vat),
            autosave: self.autosave.unwrap_or(default.autosave),
            autosave_minutes: self.autosave_minutes.unwrap_or(default.autosave_minutes),
            backup_on_close: self.backup_on_close.unwrap_or(default.backup_on_close),
            outlier_percent: self.outlier_percent.unwrap_or(default.outlier_percent),
            deviation_reference: self
                .deviation_reference
                .as_deref()
                .and_then(reference_from_key)
                .unwrap_or(default.deviation_reference),
            undercut_percent: self.undercut_percent.unwrap_or(default.undercut_percent),
            default_planner: self
                .default_planner
                .map(|address| Address {
                    name1: address.name1,
                    name2: address.name2,
                    name3: address.name3,
                    name4: address.name4,
                    street: address.street,
                    postal_code: address.postal_code,
                    city: address.city,
                    country: address.country,
                    iln: address.iln,
                    contact: address.contact,
                    phone: address.phone,
                    fax: address.fax,
                    email: address.email,
                    vat_id: address.vat_id,
                })
                .unwrap_or_default(),
            logo_file: self
                .logo_file
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
            cover_on_priced_lv: self
                .cover_on_priced_lv
                .unwrap_or(default.cover_on_priced_lv),
            page_frame: self
                .page_frame
                .map(PageFrameDto::into_frame)
                .unwrap_or_default(),
            cover_blocks: {
                let blocks = self.cover_blocks.unwrap_or_default();
                let on = |key: &str| blocks.get(key).copied().unwrap_or(true);
                proava_application::CoverBlocks {
                    logo: on("logo"),
                    addresses: on("adressen"),
                    facts: on("kenndaten"),
                    bidder: on("bieterfeld"),
                }
            },
            print_look: proava_application::PrintLook {
                style: self
                    .print_style
                    .as_deref()
                    .and_then(proava_application::PrintStyle::from_key)
                    .unwrap_or_default(),
                accent: self
                    .print_accent
                    .as_deref()
                    .and_then(proava_application::PrintLook::parse_accent)
                    .unwrap_or_else(|| proava_application::PrintLook::DEFAULT_ACCENT.to_string()),
                border: self.print_border.unwrap_or(false),
                table_grid: self.print_table_grid.unwrap_or(false),
                border_width: self
                    .print_border_width
                    .as_deref()
                    .and_then(proava_application::BorderWidth::from_key)
                    .unwrap_or_default(),
                text: proava_application::TextSpacing {
                    line: self
                        .text_line_spacing
                        .as_deref()
                        .and_then(proava_application::LineSpacing::from_key)
                        .unwrap_or_default(),
                    gap: self
                        .text_paragraph_gap
                        .as_deref()
                        .and_then(proava_application::ParagraphGap::from_key)
                        .unwrap_or_default(),
                },
            },
            print_author: self
                .print_author
                .map(|author| author.trim().to_string())
                .unwrap_or_default(),
            gaeb_images: self.gaeb_images.unwrap_or(default.gaeb_images),
            csv_windows_1252: self.csv_windows_1252.unwrap_or(default.csv_windows_1252),
            web_pages_in_system_browser: self
                .web_pages_in_system_browser
                .unwrap_or(default.web_pages_in_system_browser),
            ai_windows: self.ai_windows.unwrap_or(default.ai_windows),
            library_file: self
                .library_file
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
            library_own_window: self
                .library_own_window
                .unwrap_or(default.library_own_window),
            libraries: self
                .libraries
                .into_iter()
                .filter(|library| !library.path.trim().is_empty())
                .map(|library| LibrarySetting {
                    path: PathBuf::from(library.path),
                    label: library.label,
                    read_only: library.read_only,
                })
                .collect(),
            template_folder: self
                .template_folder
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
            spell_check: self.spell_check.unwrap_or(default.spell_check),
            dictionary_folder: self
                .dictionary_folder
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
            bidder_captions: self
                .bidder_captions
                .map(|list| proava_application::caption_list(&list.join("\n")))
                .unwrap_or_else(|| default.bidder_captions.clone()),
            owner_captions: self
                .owner_captions
                .map(|list| proava_application::caption_list(&list.join("\n")))
                .unwrap_or_else(|| default.owner_captions.clone()),
        };
        // Ein verdorbener Wert (von Hand bearbeitet) fällt auf die Vorgabe zurück.
        if preferences.problems().is_empty() {
            preferences
        } else {
            let mut repaired = default.clone();
            repaired.default_planner = preferences.default_planner.clone();
            repaired.logo_file = preferences.logo_file.clone();
            repaired.detail_visible = preferences.detail_visible;
            repaired.mark_empty_quantities = preferences.mark_empty_quantities;
            repaired.clip_previews_hidden = preferences.clip_previews_hidden;
            repaired.windows_in_front = preferences.windows_in_front;
            repaired.title_rows_filled = preferences.title_rows_filled;
            repaired.autosave = preferences.autosave;
            repaired.backup_on_close = preferences.backup_on_close;
            repaired.cover_on_priced_lv = preferences.cover_on_priced_lv;
            repaired.page_frame = preferences.page_frame.clone();
            repaired.cover_blocks = preferences.cover_blocks;
            repaired.print_look = preferences.print_look.clone();
            repaired.print_author = preferences.print_author.clone();
            repaired.gaeb_images = preferences.gaeb_images;
            repaired.csv_windows_1252 = preferences.csv_windows_1252;
            repaired.web_pages_in_system_browser = preferences.web_pages_in_system_browser;
            repaired.ai_windows = preferences.ai_windows;
            repaired.library_file = preferences.library_file.clone();
            repaired.libraries = preferences.libraries.clone();
            repaired.library_own_window = preferences.library_own_window;
            repaired.template_folder = preferences.template_folder.clone();
            repaired.spell_check = preferences.spell_check;
            repaired.dictionary_folder = preferences.dictionary_folder.clone();
            repaired
                .bidder_captions
                .clone_from(&preferences.bidder_captions);
            repaired.bidder_captions.truncate(40);
            repaired
                .owner_captions
                .clone_from(&preferences.owner_captions);
            repaired.owner_captions.truncate(40);
            repaired
        }
    }

    fn of(preferences: &Preferences) -> Self {
        let planner = &preferences.default_planner;
        Self {
            ui_zoom: Some(preferences.ui_zoom),
            detail_visible: Some(preferences.detail_visible),
            mark_empty_quantities: Some(preferences.mark_empty_quantities),
            clip_previews_hidden: preferences.clip_previews_hidden.then_some(true),
            windows_in_front: (!preferences.windows_in_front).then_some(false),
            // Only written when switched off (the default is on).
            title_rows_filled: (!preferences.title_rows_filled).then_some(false),
            new_lv_mask: Some(preferences.new_lv_mask.clone()),
            new_lv_step: Some(preferences.new_lv_step),
            new_lv_vat: Some(preferences.new_lv_vat.normalize().to_string()),
            autosave: Some(preferences.autosave),
            autosave_minutes: Some(preferences.autosave_minutes),
            backup_on_close: Some(preferences.backup_on_close),
            outlier_percent: Some(preferences.outlier_percent),
            deviation_reference: Some(reference_key(preferences.deviation_reference).to_string()),
            undercut_percent: Some(preferences.undercut_percent),
            default_planner: (*planner != Address::default()).then(|| AddressDto {
                name1: planner.name1.clone(),
                name2: planner.name2.clone(),
                name3: planner.name3.clone(),
                name4: planner.name4.clone(),
                street: planner.street.clone(),
                postal_code: planner.postal_code.clone(),
                city: planner.city.clone(),
                country: planner.country.clone(),
                iln: planner.iln.clone(),
                contact: planner.contact.clone(),
                phone: planner.phone.clone(),
                fax: planner.fax.clone(),
                email: planner.email.clone(),
                vat_id: planner.vat_id.clone(),
            }),
            logo_file: preferences
                .logo_file
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            cover_on_priced_lv: Some(preferences.cover_on_priced_lv),
            page_frame: (preferences.page_frame != proava_domain::PageFrame::default())
                .then(|| PageFrameDto::of(&preferences.page_frame)),
            cover_blocks: (preferences.cover_blocks != proava_application::CoverBlocks::default())
                .then(|| {
                    let blocks = &preferences.cover_blocks;
                    [
                        ("logo", blocks.logo),
                        ("adressen", blocks.addresses),
                        ("kenndaten", blocks.facts),
                        ("bieterfeld", blocks.bidder),
                    ]
                    .into_iter()
                    .map(|(key, on)| (key.to_string(), on))
                    .collect()
                }),
            print_style: (preferences.print_look.style
                != proava_application::PrintStyle::default())
            .then(|| preferences.print_look.style.key().to_string()),
            print_accent: (preferences.print_look.accent
                != proava_application::PrintLook::DEFAULT_ACCENT)
                .then(|| preferences.print_look.accent.clone()),
            print_author: (!preferences.print_author.is_empty())
                .then(|| preferences.print_author.clone()),
            print_border: preferences.print_look.border.then_some(true),
            print_table_grid: preferences.print_look.table_grid.then_some(true),
            print_border_width: (preferences.print_look.border_width
                != proava_application::BorderWidth::default())
            .then(|| preferences.print_look.border_width.key().to_string()),
            text_line_spacing: (preferences.print_look.text.line
                != proava_application::LineSpacing::default())
            .then(|| preferences.print_look.text.line.key().to_string()),
            text_paragraph_gap: (preferences.print_look.text.gap
                != proava_application::ParagraphGap::default())
            .then(|| preferences.print_look.text.gap.key().to_string()),
            gaeb_images: Some(preferences.gaeb_images),
            csv_windows_1252: preferences.csv_windows_1252.then_some(true),
            web_pages_in_system_browser: preferences.web_pages_in_system_browser.then_some(true),
            ai_windows: (!preferences.ai_windows).then_some(false),
            library_own_window: (!preferences.library_own_window).then_some(false),
            library_file: preferences
                .library_file
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            libraries: preferences
                .libraries
                .iter()
                .map(|library| LibraryDto {
                    path: library.path.to_string_lossy().to_string(),
                    label: library.label.clone(),
                    read_only: library.read_only,
                })
                .collect(),
            template_folder: preferences
                .template_folder
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            spell_check: Some(preferences.spell_check),
            dictionary_folder: preferences
                .dictionary_folder
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            bidder_captions: (preferences.bidder_captions
                != proava_application::default_bidder_captions())
            .then(|| preferences.bidder_captions.clone()),
            owner_captions: (preferences.owner_captions
                != proava_application::default_owner_captions())
            .then(|| preferences.owner_captions.clone()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentDto {
    name: String,
    path: String,
    #[serde(default)]
    opened_at: String,
    #[serde(default)]
    kind: String,
}

/// A width or height in points a window or column can really have.
fn sane_size(value: f32) -> bool {
    value.is_finite() && (1.0..=20_000.0).contains(&value)
}

/// Settings in `~/.config/proava/settings.json`.
pub struct JsonSettingsFile {
    path: PathBuf,
}

impl Default for JsonSettingsFile {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonSettingsFile {
    pub fn new() -> Self {
        Self {
            path: paths::config_dir().join("settings.json"),
        }
    }

    /// Settings in another file (tests, portable use).
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
}

impl JsonSettingsFile {
    /// A damaged file: keeps what can still be read (field by field, entry by
    /// entry), the rest falls back to the default. The old file is kept as
    /// `settings.json.defekt` before the next save overwrites it.
    fn salvage(&self, bytes: &[u8]) -> (SettingsDto, Vec<String>) {
        let copy = self.path.with_extension("json.defekt");
        let kept = fs::write(&copy, bytes).is_ok();
        // Broken bytes (not UTF-8) and numbers beyond f64 („1e999“) do not
        // spoil the readable rest.
        let text = without_huge_numbers(&String::from_utf8_lossy(bytes));
        let copy_note = if kept {
            format!(" Die alte Datei liegt als {} daneben.", copy.display())
        } else {
            String::new()
        };
        let Ok(serde_json::Value::Object(object)) =
            serde_json::from_str::<serde_json::Value>(&text)
        else {
            return (
                SettingsDto::default(),
                vec![format!(
                    "Die Einstellungen waren nicht lesbar und stehen wieder auf den Vorgaben \
                     (auch „Zuletzt verwendet“).{copy_note}"
                )],
            );
        };
        let mut lost = Vec::new();
        let object = readable_settings(object, &mut lost);
        let dto = serde_json::from_value(serde_json::Value::Object(object)).unwrap_or_default();
        let note = if lost.is_empty() {
            format!("Die Einstellungen waren beschädigt, alles Wichtige ist erhalten.{copy_note}")
        } else {
            format!(
                "Einige Einstellungen waren nicht lesbar und stehen wieder auf der Vorgabe: {}.{copy_note}",
                lost.join(", ")
            )
        };
        (dto, vec![note])
    }
}

/// `text` with every number outside strings that is not a finite f64
/// replaced by `null` – serde_json refuses the whole document otherwise.
fn without_huge_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    let mut in_string = false;
    while let Some((at, character)) = chars.next() {
        if in_string {
            out.push(character);
            match character {
                '\\' => {
                    if let Some((_, next)) = chars.next() {
                        out.push(next);
                    }
                }
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            out.push(character);
            continue;
        }
        if character == '-' || character.is_ascii_digit() {
            let mut end = at + character.len_utf8();
            while let Some(&(next_at, next)) = chars.peek() {
                if next.is_ascii_digit() || matches!(next, '.' | 'e' | 'E' | '+' | '-') {
                    end = next_at + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let number = &text[at..end];
            match number.parse::<f64>() {
                Ok(value) if !value.is_finite() => out.push_str("null"),
                _ => out.push_str(number),
            }
            continue;
        }
        out.push(character);
    }
    out
}

/// What a settings key is called in the program.
fn setting_label(key: &str) -> String {
    match key {
        "theme" => "Hell/Dunkel".into(),
        "recent" => "Zuletzt verwendet".into(),
        "last_dir" => "zuletzt benutzter Ordner".into(),
        "default_folder" => "Standardordner".into(),
        "detail_side" | "detail_width" => "Eigenschaftenfenster".into(),
        "column_widths" => "Spaltenbreiten".into(),
        "table_columns" => "Tabellenspalten KG/LB".into(),
        "print" => "Druckeinstellungen".into(),
        "long_text_window" => "Größe des Langtextfensters".into(),
        "transfer_sources" => "Übernahme-Quellen".into(),
        "personal_words" => "eigenes Wörterbuch".into(),
        "file_dialog_folders" => "Ordner im Dateidialog".into(),
        "last_template" => "zuletzt gewählte Projektvorlage".into(),
        "library_panel" => "Bibliotheksfenster".into(),
        "position_window" | "position_window_at" => "Größe und Lage des Positionsfensters".into(),
        other => format!("Einstellungen ({other})"),
    }
}

/// Whether `{key: value}` alone can be read as `T` (all fields optional).
fn readable<T: serde::de::DeserializeOwned>(key: &str, value: &serde_json::Value) -> bool {
    let single = serde_json::Value::Object(serde_json::Map::from_iter([(
        key.to_string(),
        value.clone(),
    )]));
    serde_json::from_value::<T>(single).is_ok()
}

/// Keeps the entries of `value` (a list or a map) that read as `T`; counts the others.
fn readable_entries<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    dropped: &mut usize,
) -> serde_json::Value {
    let ok = |entry: &serde_json::Value| serde_json::from_value::<T>(entry.clone()).is_ok();
    match value {
        serde_json::Value::Array(entries) => {
            let before = entries.len();
            let kept: Vec<_> = entries.into_iter().filter(ok).collect();
            *dropped += before - kept.len();
            serde_json::Value::Array(kept)
        }
        serde_json::Value::Object(entries) => {
            let before = entries.len();
            let kept: serde_json::Map<_, _> =
                entries.into_iter().filter(|(_, entry)| ok(entry)).collect();
            *dropped += before - kept.len();
            serde_json::Value::Object(kept)
        }
        other => other,
    }
}

/// The settings object with everything that can be read; names what was lost.
fn readable_settings(
    object: serde_json::Map<String, serde_json::Value>,
    lost: &mut Vec<String>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut kept = serde_json::Map::new();
    for (key, value) in object {
        // In a damaged file an unknown key is most likely a damaged name.
        if !KNOWN_KEYS.contains(&key.as_str()) {
            lost.push(format!(
                "unbekannter Eintrag „{}“",
                key.chars().take(30).collect::<String>()
            ));
            continue;
        }
        let mut dropped = 0;
        let value = match key.as_str() {
            "recent" => readable_entries::<RecentDto>(value, &mut dropped),
            "transfer_sources" | "personal_words" | "file_dialog_folders" => {
                readable_entries::<String>(value, &mut dropped)
            }
            "column_widths" => readable_entries::<f32>(value, &mut dropped),
            "table_columns" => readable_entries::<bool>(value, &mut dropped),
            "print" => readable_entries::<String>(value, &mut dropped),
            "preferences" => match value {
                serde_json::Value::Object(fields) => serde_json::Value::Object(
                    fields
                        .into_iter()
                        .filter(|(field, value)| {
                            let ok = readable::<PreferencesDto>(field, value);
                            if !ok {
                                dropped += 1;
                            }
                            ok
                        })
                        .collect(),
                ),
                other => other,
            },
            _ => value,
        };
        if dropped > 0 {
            lost.push(match key.as_str() {
                "preferences" => format!("{dropped} Wert(e) unter Extras › Einstellungen"),
                _ => format!("{} ({dropped} Eintrag/Einträge)", setting_label(&key)),
            });
        }
        if readable::<SettingsDto>(&key, &value) {
            kept.insert(key, value);
        } else {
            lost.push(setting_label(&key));
        }
    }
    lost.dedup();
    kept
}

/// The file read strictly: valid UTF-8 and valid JSON as a whole. serde_json
/// skips unknown keys without checking what is in them – a damaged key name
/// („#references“) with a broken value would otherwise make a whole section
/// fall back silently. Unknown keys in an intact file (a newer proAVA) stay
/// silent.
fn strict(bytes: &[u8]) -> Option<SettingsDto> {
    let text = std::str::from_utf8(bytes).ok()?;
    serde_json::from_str::<serde_json::Value>(text).ok()?;
    serde_json::from_str::<SettingsDto>(text).ok()
}

/// Keys of the settings file this version knows.
const KNOWN_KEYS: &[&str] = &[
    "theme",
    "recent",
    "last_dir",
    "default_folder",
    "detail_side",
    "detail_width",
    "column_widths",
    "table_columns",
    "print",
    "long_text_window",
    "position_window",
    "position_window_at",
    "transfer_sources",
    "file_dialog_folders",
    "preferences",
    "personal_words",
    "library_panel",
    "last_template",
];

/// The settings a (readable) file holds.
fn settings_of(dto: SettingsDto) -> Settings {
    Settings {
        theme: dto.theme.as_deref().and_then(ThemeChoice::from_key),
        recent: dto
            .recent
            .into_iter()
            .map(|entry| RecentProject {
                name: entry.name,
                path: PathBuf::from(entry.path),
                opened_at: entry.opened_at,
                kind: ProjectKind::from_key(&entry.kind).unwrap_or(ProjectKind::Other),
            })
            .collect(),
        last_dir: dto.last_dir.map(PathBuf::from),
        default_folder: dto.default_folder.map(PathBuf::from),
        detail_side: dto
            .detail_side
            .as_deref()
            .and_then(PanelSide::from_key)
            .unwrap_or_default(),
        // Sizes from a hand-edited or damaged file („1e39“ is infinite as
        // f32, negative widths) fall back to the default.
        detail_width: dto.detail_width.filter(|width| sane_size(*width)),
        column_widths: dto
            .column_widths
            .into_iter()
            .filter(|(_, width)| sane_size(*width))
            .collect(),
        table_columns: dto.table_columns,
        print: dto.print,
        long_text_window: dto
            .long_text_window
            .filter(|(width, height)| sane_size(*width) && sane_size(*height)),
        position_window: proava_application::WindowPlace {
            size: dto
                .position_window
                .filter(|(width, height)| sane_size(*width) && sane_size(*height)),
            at: dto
                .position_window_at
                .filter(|(x, y)| x.is_finite() && y.is_finite()),
        },
        transfer_sources: dto
            .transfer_sources
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        file_dialog_folders: dto
            .file_dialog_folders
            .into_iter()
            .map(PathBuf::from)
            .collect(),
        preferences: dto.preferences.into_preferences(),
        personal_words: dto.personal_words,
        library_panel: dto.library_panel,
        last_template: dto.last_template.filter(|name| !name.is_empty()),
        notes: Vec::new(),
    }
}

impl SettingsFiles for JsonSettingsFile {
    fn load(&self) -> Settings {
        // No file yet: the defaults, silently.
        let Ok(bytes) = fs::read(&self.path) else {
            return Settings::default();
        };
        let (dto, notes) = match strict(&bytes) {
            Some(dto) => (dto, Vec::new()),
            None => self.salvage(&bytes),
        };
        Settings {
            notes,
            ..settings_of(dto)
        }
    }

    /// Strict: a damaged file is no stored state – the repaired settings in
    /// memory win and are saved with the next change.
    fn load_stored(&self) -> Option<Settings> {
        let bytes = fs::read(&self.path).ok()?;
        strict(&bytes).map(settings_of)
    }

    fn save(&self, settings: &Settings) {
        let dto = SettingsDto {
            theme: settings.theme.map(|theme| theme.as_key().to_string()),
            recent: settings
                .recent
                .iter()
                .map(|entry| RecentDto {
                    name: entry.name.clone(),
                    path: entry.path.to_string_lossy().to_string(),
                    opened_at: entry.opened_at.clone(),
                    kind: entry.kind.as_key().to_string(),
                })
                .collect(),
            last_dir: settings
                .last_dir
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            default_folder: settings
                .default_folder
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            detail_side: Some(settings.detail_side.as_key().to_string()),
            detail_width: settings.detail_width,
            column_widths: settings.column_widths.clone(),
            table_columns: settings.table_columns.clone(),
            print: settings.print.clone(),
            long_text_window: settings.long_text_window,
            position_window: settings.position_window.size,
            position_window_at: settings.position_window.at,
            transfer_sources: settings
                .transfer_sources
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            file_dialog_folders: settings
                .file_dialog_folders
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            preferences: PreferencesDto::of(&settings.preferences),
            personal_words: settings.personal_words.clone(),
            library_panel: settings.library_panel,
            last_template: settings.last_template.clone(),
        };

        let Some(parent) = self.path.parent() else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        if let Ok(text) = serde_json::to_string_pretty(&dto) {
            // Atomic: a second proAVA reading at the same moment sees the
            // old or the new file, never half of it.
            let temporary = self
                .path
                .with_extension(format!("json.{}.tmp", std::process::id()));
            if fs::write(&temporary, text).is_ok()
                && crate::xml_project::replace_file(&temporary, &self.path).is_err()
            {
                let _ = fs::remove_file(&temporary);
            }
        }
    }

    fn now_iso(&self) -> String {
        chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use proava_application::{Preferences, SettingsFiles};
    use proava_domain::{Address, Decimal, DeviationReference};

    use super::JsonSettingsFile;

    /// `settings.json` in a folder only this test uses (unique, created
    /// exclusively); the folder goes with the returned guard, nothing else.
    fn temp_file(name: &str) -> (tempfile::TempDir, PathBuf) {
        let folder = tempfile::Builder::new()
            .prefix(&format!("proava-settings-{name}-"))
            .tempdir()
            .unwrap();
        let file = folder.path().join("settings.json");
        (folder, file)
    }

    #[test]
    fn old_files_get_default_preferences() {
        let (_file_folder, file) = temp_file("alt");
        std::fs::write(&file, r#"{"theme":"dark","detail_side":"right"}"#).unwrap();
        let settings = JsonSettingsFile::at(file).load();
        assert_eq!(settings.preferences, Preferences::default());
        assert_eq!(
            settings.detail_side,
            proava_application::PanelSide::Right,
            "der Rest der Datei bleibt"
        );
        assert_eq!(settings.theme, Some(proava_application::ThemeChoice::Dark));
    }

    /// Zeilenabstand and Abstand nach Absatz: stored only when not the
    /// default (einfach, kein Abstand – as AVA printouts), read back; an
    /// unknown value falls back to the default.
    #[test]
    fn text_spacing_is_stored_and_read() {
        let (_folder, file) = temp_file("textabstand");
        let store = JsonSettingsFile::at(file.clone());
        let mut settings = store.load();
        assert_eq!(
            settings.preferences.print_look.text,
            proava_application::TextSpacing::default()
        );
        store.save(&settings);
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(!written.contains("text_line_spacing"), "{written}");
        settings.preferences.print_look.text = proava_application::TextSpacing {
            line: proava_application::LineSpacing::OneHalf,
            gap: proava_application::ParagraphGap::Points6,
        };
        store.save(&settings);
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(
            written.contains("\"text_line_spacing\": \"1,5\""),
            "{written}"
        );
        assert!(
            written.contains("\"text_paragraph_gap\": \"6\""),
            "{written}"
        );
        assert_eq!(
            store.load().preferences.print_look.text,
            settings.preferences.print_look.text
        );
        std::fs::write(
            &file,
            r#"{"text_line_spacing":"doppelt","text_paragraph_gap":"x"}"#,
        )
        .unwrap();
        assert_eq!(
            store.load().preferences.print_look.text,
            proava_application::TextSpacing::default()
        );
    }

    #[test]
    fn preferences_round_trip() {
        let (_file_folder, file) = temp_file("rund");
        let store = JsonSettingsFile::at(file.clone());
        let mut settings = store.load();
        let preferences = &mut settings.preferences;
        preferences.ui_zoom = 1.25;
        preferences.detail_visible = false;
        preferences.mark_empty_quantities = false;
        preferences.title_rows_filled = false;
        preferences.new_lv_mask = "11.22.PPPP".into();
        preferences.new_lv_step = 1;
        preferences.new_lv_vat = "7.5".parse::<Decimal>().unwrap();
        preferences.autosave = false;
        preferences.autosave_minutes = 12;
        preferences.backup_on_close = true;
        preferences.outlier_percent = 30;
        preferences.deviation_reference = DeviationReference::Estimate;
        preferences.undercut_percent = 15;
        preferences.default_planner = Address {
            name1: "Planungsbüro Muster".into(),
            city: "Musterstadt".into(),
            ..Address::default()
        };
        preferences.logo_file = Some(PathBuf::from("/tmp/logo.png"));
        preferences.cover_on_priced_lv = true;
        preferences.page_frame.header[0] = "{projektnummer}\nStand {stand}".into();
        preferences.page_frame.footer[2] = "S. {seite}".into();
        preferences.page_frame.header_rule = false;
        preferences.page_frame.logo = proava_domain::FrameLogoPlace::Right;
        preferences.page_frame.font_size_pt = 9;
        preferences.cover_blocks.bidder = false;
        preferences.cover_blocks.logo = false;
        preferences.print_look = proava_application::PrintLook {
            style: proava_application::PrintStyle::ProAva,
            accent: "#123ABC".into(),
            border: true,
            border_width: proava_application::BorderWidth::Medium,
            table_grid: true,
            text: proava_application::TextSpacing {
                line: proava_application::LineSpacing::OneFifteen,
                gap: proava_application::ParagraphGap::HalfLine,
            },
        };
        preferences.print_author = "Max Beispiel".into();
        preferences.csv_windows_1252 = true;
        preferences.web_pages_in_system_browser = true;
        preferences.library_own_window = false;
        preferences.bidder_captions =
            vec!["Fabrikat:".into(), "Raumbediengerät, Fabrikat/Typ:".into()];
        store.save(&settings);
        assert_eq!(store.load().preferences, settings.preferences);
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("\"csv_windows_1252\": true"));
        assert!(written.contains("\"web_pages_in_system_browser\": true"));
        assert!(written.contains("\"title_rows_filled\": false"));
        assert!(written.contains("\"library_own_window\": false"));
        assert!(written.contains("\"print_table_grid\": true"));
        assert!(written.contains("Raumbediengerät, Fabrikat/Typ:"));
        assert!(
            !written.contains("owner_captions"),
            "Vorgabe wird nicht geschrieben"
        );
    }

    /// Schnellauswahl der Bieterangaben: ohne Eintrag die Vorgabe für GA,
    /// leere Zeilen und Doppelte fallen weg.
    #[test]
    fn complement_captions_default_and_cleanup() {
        let (_file_folder, file) = temp_file("vortexte");
        std::fs::write(&file, r#"{"preferences":{"autosave":false}}"#).unwrap();
        let preferences = JsonSettingsFile::at(file.clone()).load().preferences;
        assert_eq!(
            preferences.bidder_captions,
            proava_application::default_bidder_captions()
        );
        assert!(preferences
            .bidder_captions
            .iter()
            .any(|caption| caption == "Fabrikat:"));
        std::fs::write(
            &file,
            r#"{"preferences":{"bidder_captions":["Typ:", " ", "Typ:", " Hersteller: "],"owner_captions":[]}}"#,
        )
        .unwrap();
        let preferences = JsonSettingsFile::at(file).load().preferences;
        assert_eq!(preferences.bidder_captions, ["Typ:", "Hersteller:"]);
        assert!(
            preferences.owner_captions.is_empty(),
            "leere Liste bleibt leer"
        );
    }

    /// Offener Punkt aus den Eigenschaftstests: ein einziger kaputter Wert
    /// setzte alle Einstellungen zurück, auch „Zuletzt verwendet“. Jetzt
    /// bleibt alles Lesbare, die alte Datei liegt daneben, ein Hinweis sagt es.
    #[test]
    fn damaged_file_keeps_what_can_be_read() {
        let (_file_folder, file) = temp_file("teilweise");
        std::fs::write(
            &file,
            r#"{"theme":"dark","detail_side":5,"detail_width":"breit",
                "recent":[{"name":"Schule","path":"/p/Schule.proava","opened_at":"2026-11-17T10:00:00","kind":"proava"},
                          {"name":"Kaputt","path":7}],
                "column_widths":{"oz":120.0,"ep":"x"},
                "preferences":{"ui_zoom":"groß","autosave":false,"new_lv_step":10}}"#,
        )
        .unwrap();
        let settings = JsonSettingsFile::at(file.clone()).load();
        assert_eq!(settings.theme, Some(proava_application::ThemeChoice::Dark));
        assert_eq!(settings.recent.len(), 1, "der lesbare Eintrag bleibt");
        assert_eq!(settings.recent[0].name, "Schule");
        assert_eq!(settings.column_widths.get("oz"), Some(&120.0));
        assert!(!settings.preferences.autosave, "lesbare Einstellung bleibt");
        assert_eq!(
            settings.preferences.ui_zoom, 1.0,
            "kaputter Wert auf Vorgabe"
        );
        assert_eq!(settings.detail_width, None);
        assert_eq!(settings.notes.len(), 1, "{:?}", settings.notes);
        let note = &settings.notes[0];
        for part in [
            "Zuletzt verwendet (1",
            "Spaltenbreiten (1",
            "Eigenschaftenfenster",
            "unter Extras › Einstellungen",
            "settings.json.defekt",
        ] {
            assert!(note.contains(part), "„{part}“ fehlt: {note}");
        }
        assert!(
            file.with_extension("json.defekt").is_file(),
            "alte Datei daneben"
        );
    }

    #[test]
    fn table_columns_are_kept_and_damaged_entries_dropped() {
        let (_file_folder, file) = temp_file("spalten");
        let store = JsonSettingsFile::at(file.clone());
        let mut settings = store.load();
        assert!(settings.table_columns.is_empty(), "Vorgabe: nichts gemerkt");
        settings.table_columns.insert("lb".into(), true);
        settings.table_columns.insert("kg".into(), false);
        store.save(&settings);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("\"table_columns\""), "{text}");
        assert_eq!(store.load().table_columns, settings.table_columns);
        std::fs::write(&file, r#"{"table_columns":{"kg":false,"lb":"ja"}}"#).unwrap();
        let loaded = JsonSettingsFile::at(file).load();
        assert_eq!(loaded.table_columns.get("kg"), Some(&false));
        assert_eq!(loaded.table_columns.get("lb"), None, "kaputter Eintrag weg");
        assert!(
            loaded.notes[0].contains("Tabellenspalten KG/LB"),
            "{:?}",
            loaded.notes
        );
    }

    #[test]
    fn unreadable_file_falls_back_with_a_note_and_a_copy() {
        let (_file_folder, file) = temp_file("abgeschnitten");
        std::fs::write(&file, r#"{"theme":"dark","recent":[{"name":"Sch"#).unwrap();
        let settings = JsonSettingsFile::at(file.clone()).load();
        assert!(settings.recent.is_empty());
        assert!(
            settings.notes[0].contains("nicht lesbar"),
            "{:?}",
            settings.notes
        );
        assert_eq!(
            std::fs::read_to_string(file.with_extension("json.defekt")).unwrap(),
            r#"{"theme":"dark","recent":[{"name":"Sch"#
        );
        // A damaged byte in a key the program does not know: said too.
        let mut bytes = br#"{"theme":"dark","th"#.to_vec();
        bytes.extend_from_slice(&[0x80]);
        bytes.extend_from_slice(br#"x":1}"#);
        std::fs::write(&file, &bytes).unwrap();
        let settings = JsonSettingsFile::at(file.clone()).load();
        assert_eq!(settings.theme, Some(proava_application::ThemeChoice::Dark));
        assert_eq!(settings.notes.len(), 1, "Hinweis bei ungültigem Byte");
        assert!(
            settings.notes[0].contains("unbekannter Eintrag"),
            "{:?}",
            settings.notes
        );
        // A damaged key name with a broken value: the section is named.
        std::fs::write(
            &file,
            r##"{"theme":"dark","#references":{"ui_zoom":1e999}}"##,
        )
        .unwrap();
        let settings = JsonSettingsFile::at(file.clone()).load();
        assert_eq!(settings.theme, Some(proava_application::ThemeChoice::Dark));
        assert!(
            settings.notes[0].contains("„#references“"),
            "{:?}",
            settings.notes
        );
        // Unknown keys in an intact file (a newer proAVA): silent.
        std::fs::write(&file, r#"{"theme":"dark","neu_in_2027":{"x":1}}"#).unwrap();
        assert!(JsonSettingsFile::at(file.clone()).load().notes.is_empty());
        // Not UTF-8 (one damaged byte): the readable rest stays.
        let mut bytes =
            br#"{"theme":"dark","recent":[{"name":"Schule","path":"/p/S.proava"}],"last_dir":"/x"#
                .to_vec();
        bytes.extend_from_slice(&[0x80, b'"', b'}']);
        std::fs::write(&file, &bytes).unwrap();
        let settings = JsonSettingsFile::at(file.clone()).load();
        assert_eq!(settings.recent.len(), 1, "{:?}", settings.notes);
        assert_eq!(settings.notes.len(), 1);
        assert_eq!(
            std::fs::read(file.with_extension("json.defekt")).unwrap(),
            bytes
        );
        // An intact file says nothing.
        let store = JsonSettingsFile::at(file.clone());
        store.save(&settings);
        assert!(store.load().notes.is_empty());
    }

    /// The Vorgabe 11.222.PPPPI only fills a missing entry; a mask stored in
    /// the file (also the former Vorgabe 11.22.PPPPI) stays, also after the
    /// next save.
    #[test]
    fn a_stored_mask_wins_over_the_new_default() {
        assert_eq!(Preferences::default().new_lv_mask, "11.222.PPPPI");

        let (_missing_folder, missing) = temp_file("maske-fehlt");
        std::fs::write(&missing, r#"{"preferences":{"new_lv_step":10}}"#).unwrap();
        assert_eq!(
            JsonSettingsFile::at(missing).load().preferences.new_lv_mask,
            "11.222.PPPPI"
        );

        let (_stored_folder, stored) = temp_file("maske-gespeichert");
        std::fs::write(
            &stored,
            r#"{"preferences":{"new_lv_mask":"11.22.PPPPI","new_lv_step":10,"new_lv_vat":"19"}}"#,
        )
        .unwrap();
        let store = JsonSettingsFile::at(stored.clone());
        let mut settings = store.load();
        assert_eq!(
            settings.preferences.new_lv_mask, "11.22.PPPPI",
            "ausdrücklich gespeichert"
        );
        settings.preferences.ui_zoom = 1.1;
        store.save(&settings);
        assert_eq!(
            store.load().preferences.new_lv_mask,
            "11.22.PPPPI",
            "bleibt beim Speichern"
        );
        let text = std::fs::read_to_string(&stored).unwrap();
        assert!(text.contains(r#""new_lv_mask": "11.22.PPPPI""#), "{text}");
    }

    #[test]
    fn file_dialog_folders_round_trip() {
        let (_file_folder, file) = temp_file("ordner");
        let store = JsonSettingsFile::at(file.clone());
        let mut settings = store.load();
        assert!(
            settings.file_dialog_folders.is_empty(),
            "alte Dateien ohne Eintrag"
        );
        settings.file_dialog_folders = vec![PathBuf::from("/a/Logos"), PathBuf::from("/b")];
        store.save(&settings);
        assert_eq!(
            store.load().file_dialog_folders,
            settings.file_dialog_folders
        );
    }

    #[test]
    fn template_folder_and_last_template_round_trip() {
        let (_file_folder, file) = temp_file("vorlagen");
        let store = JsonSettingsFile::at(file.clone());
        let mut settings = store.load();
        assert_eq!(
            settings.preferences.template_folder, None,
            "Vorgabe: Home-Ordner"
        );
        assert_eq!(settings.last_template, None);
        settings.preferences.template_folder = Some(PathBuf::from("/a/proAVA-Vorlagen"));
        settings.last_template = Some("GA Schulen".into());
        store.save(&settings);
        let loaded = store.load();
        assert_eq!(
            loaded.preferences.template_folder,
            settings.preferences.template_folder
        );
        assert_eq!(loaded.last_template.as_deref(), Some("GA Schulen"));
    }

    #[test]
    fn position_window_place_round_trip() {
        let (_file_folder, file) = temp_file("posfenster");
        let store = JsonSettingsFile::at(file);
        let mut settings = store.load();
        assert_eq!(settings.position_window, Default::default());
        settings.position_window = proava_application::WindowPlace {
            size: Some((1100.0, 780.0)),
            at: Some((2600.0, 140.0)),
        };
        store.save(&settings);
        assert_eq!(store.load().position_window, settings.position_window);
    }

    /// `KNOWN_KEYS` covers every key the file can hold. The literal lists every
    /// field without `..Default::default()`: a new field does not compile
    /// here until it is added – then also to `KNOWN_KEYS`.
    #[test]
    fn known_keys_match_the_settings_file() {
        let full = super::SettingsDto {
            theme: Some(String::new()),
            recent: Vec::new(),
            last_dir: Some(String::new()),
            default_folder: Some(String::new()),
            detail_side: Some(String::new()),
            detail_width: Some(1.0),
            column_widths: Default::default(),
            table_columns: [("kg".to_string(), true)].into(),
            print: Default::default(),
            long_text_window: Some((1.0, 1.0)),
            position_window: Some((1.0, 1.0)),
            position_window_at: Some((1.0, 1.0)),
            transfer_sources: vec![String::new()],
            file_dialog_folders: vec![String::new()],
            preferences: Default::default(),
            personal_words: vec![String::new()],
            library_panel: true,
            last_template: Some(String::new()),
        };
        let serde_json::Value::Object(object) = serde_json::to_value(&full).unwrap() else {
            panic!("kein Objekt");
        };
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        let mut known = super::KNOWN_KEYS.to_vec();
        keys.sort();
        known.sort();
        assert_eq!(keys, known);
    }

    #[test]
    fn huge_numbers_become_null_outside_strings() {
        assert_eq!(
            super::without_huge_numbers(r#"{"a":1e999,"b":"1e999","c":-2.5e3,"d":[1E400,-1e999]}"#),
            r#"{"a":null,"b":"1e999","c":-2.5e3,"d":[null,null]}"#
        );
        assert_eq!(
            super::without_huge_numbers(r#"{"x":"a\"1e999"}"#),
            r#"{"x":"a\"1e999"}"#
        );
    }

    #[test]
    fn broken_values_fall_back_to_defaults() {
        let (_file_folder, file) = temp_file("kaputt");
        std::fs::write(
            &file,
            r#"{"preferences":{"ui_zoom":9.0,"autosave":false,"new_lv_vat":"x"}}"#,
        )
        .unwrap();
        let preferences = JsonSettingsFile::at(file).load().preferences;
        assert_eq!(preferences.ui_zoom, 1.0);
        assert!(!preferences.autosave);
        assert_eq!(preferences.new_lv_vat, Decimal::from(19));
    }
}
