//! Extras ▸ Einstellungen: one dialog for the start screen and the workspace.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use proava_application::{Launcher, PanelSide, Preferences, ThemeChoice};
use proava_domain::{format_decimal_de, parse_decimal_de, Address, DeviationReference, OzMask};

use crate::file_dialog::{DialogResult, FileDialog};
use crate::form_kit;
use crate::theme::Palette;
use crate::{strings, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    NewProjects,
    Saving,
    Comparison,
    Print,
    PageFrame,
}

const TABS: [(SettingsTab, &str); 6] = [
    (SettingsTab::Appearance, "Darstellung"),
    (SettingsTab::NewProjects, "Neue Projekte"),
    (SettingsTab::Saving, "Speichern"),
    (SettingsTab::Comparison, "Preisspiegel"),
    (SettingsTab::Print, "Drucken"),
    (SettingsTab::PageFrame, "Kopf-/Fußzeile"),
];

/// What happened with the dialog this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsResult {
    Open,
    Cancelled,
    /// Taken over and stored; the caller refreshes theme and views.
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickTarget {
    Folder,
    Logo,
    Library,
    Templates,
    Dictionary,
}

/// Form state of the settings dialog; written only on „Übernehmen“.
pub struct SettingsDialog {
    tab: SettingsTab,
    pub theme: ThemeChoice,
    pub zoom_percent: u32,
    pub side: PanelSide,
    pub detail_visible: bool,
    pub reset_width: bool,
    pub mark_empty: bool,
    /// Vorschau aller PDF-Ausschnitte ausblenden.
    pub clip_previews_hidden: bool,
    pub windows_in_front: bool,
    /// Titelzeilen farbig hinterlegen.
    pub title_rows_filled: bool,
    pub default_folder: String,
    pub mask: String,
    pub step: u32,
    pub vat: String,
    pub autosave: bool,
    pub autosave_minutes: u32,
    pub backup_on_close: bool,
    pub outlier: u32,
    pub reference: DeviationReference,
    pub undercut: u32,
    pub planner: Address,
    pub logo: String,
    pub cover_priced: bool,
    /// Blocks of the Deckblatt.
    pub cover_blocks: proava_application::CoverBlocks,
    /// Default Druckstil of all kinds.
    pub print_style: proava_application::PrintStyle,
    /// Blattrahmen by default and its width.
    pub print_border: bool,
    pub print_border_width: proava_application::BorderWidth,
    pub print_table_grid: bool,
    /// Accent colour of the printouts as typed („#004F7C“).
    pub print_accent: String,
    /// Zeilenabstand and Abstand nach Absatz of the texts (editor and print).
    pub text_spacing: proava_application::TextSpacing,
    /// Bearbeiter (person) for `{ersteller}`/`{bearbeiter}`.
    pub print_author: String,
    /// Kopf-/Fußzeile of all printouts (a project may have its own).
    pub page_frame: proava_domain::PageFrame,
    /// GAEB-Export: Bilder der Langtexte mitgeben.
    pub gaeb_images: bool,
    /// CSV-Export in Windows-1252 statt UTF-8.
    pub csv_windows_1252: bool,
    /// AUSSCHREIBEN.DE and STLB-Bau Online in the system browser.
    pub web_pages_in_system_browser: bool,
    /// KI-Assistenten dürfen geöffnete Fenster bedienen (Brücke).
    pub ai_windows: bool,
    /// Own position library (empty = default file in the home folder).
    pub library: String,
    /// Further libraries: path, name, read-only.
    pub libraries: Vec<(String, String, bool)>,
    /// „Bibliothek bearbeiten …“ in einem eigenen Fenster.
    pub library_own_window: bool,
    /// Folder of the own project templates (empty = proAVA-Vorlagen in the home folder).
    pub template_folder: String,
    pub spell_check: bool,
    /// Folder with de_DE.aff/.dic (empty = search the system).
    pub dictionary_folder: String,
    /// Quick choice of the Textergänzungen, one Vortext per line.
    pub bidder_captions: String,
    pub owner_captions: String,
    errors: Vec<String>,
    picker: Option<(FileDialog, PickTarget)>,
}

impl SettingsDialog {
    pub fn of(launcher: &Launcher) -> Self {
        let preferences = launcher.preferences();
        Self {
            tab: SettingsTab::Appearance,
            theme: launcher.theme(),
            zoom_percent: (preferences.ui_zoom * 100.0).round() as u32,
            side: launcher.detail_side(),
            detail_visible: preferences.detail_visible,
            reset_width: false,
            mark_empty: preferences.mark_empty_quantities,
            clip_previews_hidden: preferences.clip_previews_hidden,
            windows_in_front: preferences.windows_in_front,
            title_rows_filled: preferences.title_rows_filled,
            default_folder: launcher
                .default_folder()
                .map(|folder| folder.to_string_lossy().to_string())
                .unwrap_or_default(),
            mask: preferences.new_lv_mask.clone(),
            step: preferences.new_lv_step,
            vat: vat_text(preferences.new_lv_vat),
            autosave: preferences.autosave,
            autosave_minutes: preferences.autosave_minutes,
            backup_on_close: preferences.backup_on_close,
            outlier: preferences.outlier_percent,
            reference: preferences.deviation_reference,
            undercut: preferences.undercut_percent,
            planner: preferences.default_planner.clone(),
            logo: preferences
                .logo_file
                .as_ref()
                .map(|file| file.to_string_lossy().to_string())
                .unwrap_or_default(),
            cover_priced: preferences.cover_on_priced_lv,
            cover_blocks: preferences.cover_blocks,
            print_style: preferences.print_look.style,
            print_border: preferences.print_look.border,
            print_border_width: preferences.print_look.border_width,
            print_table_grid: preferences.print_look.table_grid,
            print_accent: preferences.print_look.accent.clone(),
            text_spacing: preferences.print_look.text,
            print_author: preferences.print_author.clone(),
            page_frame: preferences.page_frame.clone(),
            gaeb_images: preferences.gaeb_images,
            csv_windows_1252: preferences.csv_windows_1252,
            web_pages_in_system_browser: preferences.web_pages_in_system_browser,
            ai_windows: preferences.ai_windows,
            library: preferences
                .library_file
                .as_ref()
                .map(|file| file.to_string_lossy().to_string())
                .unwrap_or_default(),
            libraries: preferences
                .libraries
                .iter()
                .map(|library| {
                    (
                        library.path.to_string_lossy().to_string(),
                        library.label.clone(),
                        library.read_only,
                    )
                })
                .collect(),
            library_own_window: preferences.library_own_window,
            template_folder: preferences
                .template_folder
                .as_ref()
                .map(|folder| folder.to_string_lossy().to_string())
                .unwrap_or_default(),
            spell_check: preferences.spell_check,
            dictionary_folder: preferences
                .dictionary_folder
                .as_ref()
                .map(|folder| folder.to_string_lossy().to_string())
                .unwrap_or_default(),
            bidder_captions: preferences.bidder_captions.join("\n"),
            owner_captions: preferences.owner_captions.join("\n"),
            errors: Vec::new(),
            picker: None,
        }
    }

    /// Opens the dialog on a tab (e.g. Darstellung from the Ansicht menu).
    pub fn on_tab(mut self, tab: SettingsTab) -> Self {
        self.tab = tab;
        self
    }

    /// The form as preferences plus default folder; problems otherwise.
    /// `is_folder` and `is_file` check typed paths.
    pub fn to_preferences(
        &self,
        is_folder: impl Fn(&Path) -> bool,
        is_file: impl Fn(&Path) -> bool,
    ) -> Result<(Preferences, Option<PathBuf>), Vec<String>> {
        let mut problems = Vec::new();
        let vat = match parse_decimal_de(self.vat.trim().trim_end_matches('%').trim()) {
            Ok(vat) => vat,
            Err(_) => {
                problems.push(format!(
                    "„{}“ ist keine Zahl für die Umsatzsteuer.",
                    self.vat.trim()
                ));
                proava_domain::Decimal::from(19)
            }
        };
        let folder = self.default_folder.trim();
        let folder = if folder.is_empty() {
            None
        } else if is_folder(Path::new(folder)) {
            Some(PathBuf::from(folder))
        } else {
            problems.push(format!("Den Standardordner „{folder}“ gibt es nicht."));
            None
        };
        let logo = self.logo.trim();
        let logo = if logo.is_empty() {
            None
        } else if is_file(Path::new(logo)) {
            let lower = logo.to_lowercase();
            if !(lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")) {
                problems.push("Das Logo muss ein PNG- oder JPEG-Bild sein.".to_string());
            }
            Some(PathBuf::from(logo))
        } else {
            problems.push(format!("Die Logo-Datei „{logo}“ gibt es nicht."));
            None
        };
        let library = self.library.trim();
        let library = if library.is_empty() {
            None
        } else {
            let path = PathBuf::from(library);
            if !library.to_lowercase().ends_with(".proava") {
                problems.push("Die Bibliothek ist eine proAVA-Datei (.proava).".to_string());
            } else if !path.parent().is_some_and(&is_folder) {
                problems.push(format!(
                    "Den Ordner der Bibliothek „{library}“ gibt es nicht."
                ));
            }
            Some(path)
        };
        let mut libraries = Vec::new();
        for (path, label, read_only) in &self.libraries {
            let path = path.trim();
            if path.is_empty() {
                continue;
            }
            if !path.to_lowercase().ends_with(".proava") {
                problems.push(format!(
                    "Die Bibliothek „{path}“ ist keine proAVA-Datei (.proava)."
                ));
            }
            libraries.push(proava_application::LibrarySetting {
                path: PathBuf::from(path),
                label: label.trim().to_string(),
                read_only: *read_only,
            });
        }
        // The template folder may not exist yet: „Als Vorlage speichern …“ creates it.
        let templates = self.template_folder.trim();
        let template_folder = (!templates.is_empty()).then(|| PathBuf::from(templates));
        let dictionary = self.dictionary_folder.trim();
        let dictionary_folder = if dictionary.is_empty() {
            None
        } else if is_folder(Path::new(dictionary)) {
            Some(PathBuf::from(dictionary))
        } else {
            problems.push(format!(
                "Den Wörterbuch-Ordner „{dictionary}“ gibt es nicht."
            ));
            None
        };
        let accent = match proava_application::PrintLook::parse_accent(&self.print_accent) {
            Some(accent) => accent,
            None => {
                problems.push(format!(
                    "„{}“ ist keine Akzentfarbe; bitte als #RRGGBB, z. B. {}.",
                    self.print_accent.trim(),
                    proava_application::PrintLook::DEFAULT_ACCENT
                ));
                proava_application::PrintLook::DEFAULT_ACCENT.to_string()
            }
        };
        let preferences = Preferences {
            ui_zoom: self.zoom_percent as f32 / 100.0,
            detail_visible: self.detail_visible,
            mark_empty_quantities: self.mark_empty,
            clip_previews_hidden: self.clip_previews_hidden,
            windows_in_front: self.windows_in_front,
            title_rows_filled: self.title_rows_filled,
            new_lv_mask: self.mask.trim().to_string(),
            new_lv_step: self.step,
            new_lv_vat: vat,
            autosave: self.autosave,
            autosave_minutes: self.autosave_minutes,
            backup_on_close: self.backup_on_close,
            outlier_percent: self.outlier,
            deviation_reference: self.reference,
            undercut_percent: self.undercut,
            default_planner: form_kit::trimmed(&self.planner),
            logo_file: logo,
            cover_on_priced_lv: self.cover_priced,
            cover_blocks: self.cover_blocks,
            print_look: proava_application::PrintLook {
                style: self.print_style,
                accent,
                border: self.print_border,
                border_width: self.print_border_width,
                table_grid: self.print_table_grid,
                text: self.text_spacing,
            },
            print_author: self.print_author.trim().to_string(),
            page_frame: self.page_frame.normalized(),
            gaeb_images: self.gaeb_images,
            csv_windows_1252: self.csv_windows_1252,
            web_pages_in_system_browser: self.web_pages_in_system_browser,
            ai_windows: self.ai_windows,
            library_file: library,
            libraries,
            library_own_window: self.library_own_window,
            template_folder,
            spell_check: self.spell_check,
            dictionary_folder,
            bidder_captions: proava_application::caption_list(&self.bidder_captions),
            owner_captions: proava_application::caption_list(&self.owner_captions),
        };
        problems.extend(preferences.problems());
        if problems.is_empty() {
            Ok((preferences, folder))
        } else {
            Err(problems)
        }
    }

    /// Stores everything; problems stay in the dialog.
    pub fn apply(&mut self, launcher: &Launcher) -> Result<(), Vec<String>> {
        let (preferences, folder) = self.to_preferences(
            |path| launcher.is_folder(path),
            |path| launcher.read_file(path).is_ok(),
        )?;
        launcher.set_preferences(preferences)?;
        launcher.set_default_folder(folder);
        launcher.set_theme(self.theme);
        launcher.set_detail_side(self.side);
        if self.reset_width {
            launcher.reset_detail_width();
        }
        Ok(())
    }

    /// Draws the dialog (or its file picker) and stores on „Übernehmen“.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
    ) -> SettingsResult {
        if let Some((picker, target)) = &mut self.picker {
            match picker.show(ui, palette, launcher) {
                DialogResult::Open => {}
                DialogResult::Cancelled => self.picker = None,
                DialogResult::Chosen(paths) => {
                    if let Some(path) = paths.first() {
                        let text = path.to_string_lossy().to_string();
                        match target {
                            PickTarget::Folder => self.default_folder = text,
                            PickTarget::Logo => self.logo = text,
                            PickTarget::Library => self.library = text,
                            PickTarget::Templates => self.template_folder = text,
                            PickTarget::Dictionary => self.dictionary_folder = text,
                        }
                    }
                    self.picker = None;
                }
            }
            return SettingsResult::Open;
        }

        let mut pick = None;
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 640.0, |ui| {
            widgets::title(
                ui,
                palette,
                strings::DIALOG_SETTINGS_TITLE,
                "Gelten für alle Projekte und bleiben nach einem Neustart erhalten.",
            );
            form_kit::tab_bar(ui, palette, &TABS, &mut self.tab);
            egui::ScrollArea::vertical()
                .max_height((ui.ctx().content_rect().height() * 0.62).min(520.0))
                .min_scrolled_height((ui.ctx().content_rect().height() * 0.62).min(520.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_height((ui.ctx().content_rect().height() * 0.62).min(520.0) - 4.0);
                    match self.tab {
                        SettingsTab::Appearance => pick = self.appearance(ui, palette).or(pick),
                        SettingsTab::NewProjects => pick = self.new_projects(ui, palette).or(pick),
                        SettingsTab::Saving => self.saving(ui, palette),
                        SettingsTab::Comparison => self.comparison(ui, palette),
                        SettingsTab::Print => pick = self.print(ui, palette).or(pick),
                        SettingsTab::PageFrame => {
                            ui.label(
                                RichText::new(
                                    "Gilt für alle Ausdrucke; ein Projekt kann eine eigene haben (Menü Projekt, Projektstammdaten). Briefe haben einen eigenen Briefkopf und nutzen nur die Fußzeile.",
                                )
                                .size(12.0)
                                .color(palette.text_dim),
                            );
                            ui.add_space(6.0);
                            crate::frame_form::editor(ui, palette, &mut self.page_frame, &crate::frame_form::sample_value);
                        }
                    }
                });
            ui.add_space(8.0);
            for error in &self.errors {
                ui.label(RichText::new(error).size(12.5).color(palette.error));
            }
            if !self.errors.is_empty() {
                ui.add_space(6.0);
            }
            widgets::dialog_buttons(ui, palette, "Übernehmen")
        });

        if let Some(target) = pick {
            let start = |text: &str, file: bool| start_folder(launcher, text, file);
            let dialog = match target {
                PickTarget::Folder => {
                    FileDialog::folder(launcher, strings::PICKER_TITLE, start(&self.default_folder, false))
                }
                PickTarget::Templates => FileDialog::folder(
                    launcher,
                    "Ordner der Projektvorlagen",
                    start(&self.template_folder, false),
                ),
                PickTarget::Dictionary => FileDialog::folder(
                    launcher,
                    "Ordner mit dem Wörterbuch (de_DE.aff/.dic)",
                    start(&self.dictionary_folder, false),
                ),
                PickTarget::Logo => FileDialog::open(
                    launcher,
                    "Logo wählen",
                    "PNG oder JPEG für die Kopfzeile der Ausdrucke. Links Orte, Laufwerke und zuletzt benutzte Ordner; oben lässt sich der Pfad auch eintippen.",
                    start(&self.logo, true),
                    &strings::IMAGE_EXTENSIONS,
                    false,
                )
                .with_selection(Path::new(self.logo.trim())),
                PickTarget::Library => {
                    let current = PathBuf::from(self.library.trim());
                    let name = if self.library.trim().is_empty() {
                        "proAVA-Bibliothek.proava".to_string()
                    } else {
                        crate::file_dialog::file_name(&current)
                    };
                    let folder = if self.library.trim().is_empty() {
                        launcher.home_folder()
                    } else {
                        start(&self.library, true)
                    };
                    FileDialog::choose_file(
                        launcher,
                        "Positionsbibliothek wählen",
                        "Vorhandene Bibliothek wählen oder einen neuen Namen eintragen; „In Bibliothek übernehmen …“ legt sie beim ersten Mal an.",
                        folder,
                        &name,
                        "proava",
                    )
                }
            };
            self.picker = Some((dialog, target));
            return SettingsResult::Open;
        }
        if close || cancelled {
            return SettingsResult::Cancelled;
        }
        if confirmed {
            return match self.apply(launcher) {
                Ok(()) => SettingsResult::Applied,
                Err(errors) => {
                    self.errors = errors;
                    SettingsResult::Open
                }
            };
        }
        SettingsResult::Open
    }

    fn appearance(&mut self, ui: &mut egui::Ui, palette: &Palette) -> Option<PickTarget> {
        form_kit::section(ui, palette, "Aussehen");
        form_kit::choice(
            ui,
            palette,
            &[
                (ThemeChoice::Light, strings::THEME_LIGHT),
                (ThemeChoice::Dark, strings::THEME_DARK),
            ],
            &mut self.theme,
        );
        form_kit::section(ui, palette, "Schriftgröße der Oberfläche");
        ui.horizontal(|ui| {
            ui.add(
                egui::Slider::new(&mut self.zoom_percent, 70..=200)
                    .step_by(5.0)
                    .suffix(" %"),
            );
            if ui.button("Zurück auf 100 %").clicked() {
                self.zoom_percent = 100;
            }
        });
        form_kit::hint(
            ui,
            palette,
            "Vergrößert Schrift und Bedienelemente (auch Strg+Plus/Minus).",
        );
        ui.add_space(8.0);
        self.text_spacing_ui(ui, palette);
        ui.add_space(8.0);
        form_kit::section(ui, palette, "Eigenschaftenfenster im LV");
        form_kit::choice(
            ui,
            palette,
            &[
                (PanelSide::Left, strings::SIDE_LEFT),
                (PanelSide::Right, strings::SIDE_RIGHT),
            ],
            &mut self.side,
        );
        ui.checkbox(&mut self.detail_visible, "Eigenschaftenfenster anzeigen");
        ui.checkbox(
            &mut self.reset_width,
            "Breite des Eigenschaftenfensters zurücksetzen",
        );
        ui.checkbox(
            &mut self.windows_in_front,
            "Positions- und Langtextfenster vor dem Hauptfenster halten",
        )
        .on_hover_text(
            "Die Fenster „Eigenschaften“ und „Langtext“ gehören zum Hauptfenster und bleiben vor ihm sichtbar; ein anderes Programm kommt beim Anklicken oder mit Alt+Tab davor",
        );
        ui.add_space(8.0);
        form_kit::section(ui, palette, "Markierungen");
        ui.checkbox(&mut self.title_rows_filled, "Titelzeilen farbig hinterlegen")
            .on_hover_text(
                "Titel und Untertitel in LV-Tabelle, Preisspiegel und Abrechnung gelb hinterlegt (Untertitel heller)",
            );
        ui.checkbox(&mut self.mark_empty, "Positionen ohne Menge markieren");
        ui.add_space(8.0);
        form_kit::section(ui, palette, "PDF-Ausschnitte im Langtext");
        ui.checkbox(&mut self.clip_previews_hidden, "Vorschau aller PDF-Ausschnitte ausblenden")
            .on_hover_text(
                "Nur Platzhalter mit Name und Größe – spart Rechenzeit bei großen Plänen; der Druck enthält die Ausschnitte",
            );
        ui.add_space(8.0);
        form_kit::section(ui, palette, "Rechtschreibung");
        ui.checkbox(
            &mut self.spell_check,
            "Rechtschreibung prüfen (rote Wellenlinie, Rechtsklick für Vorschläge)",
        );
        let mut pick = None;
        if form_kit::path_field(
            ui,
            palette,
            "Wörterbuch-Ordner (leer = im System suchen)",
            &mut self.dictionary_folder,
            "z. B. C:\\proAVA\\dictionaries",
            strings::CHOOSE_FOLDER,
        ) {
            pick = Some(PickTarget::Dictionary);
        }
        form_kit::hint(ui, palette, &crate::spell::status_text());
        form_kit::hint(
            ui,
            palette,
            "Benötigt ein deutsches Hunspell-Wörterbuch (de_DE.aff und de_DE.dic): unter Linux \
             das Paket „hunspell-de-de“; unter Windows nimmt proAVA die Wörterbücher von \
             LibreOffice, wenn es installiert ist. Sonst die Dateien in den Ordner \
             „dictionaries“ neben dem Programm oder in den gewählten Ordner legen. \
             Fachwörter der Gebäudeautomation kennt proAVA selbst.",
        );
        ui.add_space(8.0);
        self.complement_captions(ui, palette);
        ui.add_space(8.0);
        form_kit::section(ui, palette, "KI-Assistenten");
        ui.checkbox(
            &mut self.ai_windows,
            "KI darf geöffnete Fenster bedienen (zeigen, markieren, suchen, Zeilen einfügen)",
        );
        form_kit::hint(
            ui,
            palette,
            "Ein KI-Assistent (MCP-Server proava-mcp) arbeitet dann in diesem Fenster statt in der Datei: \
             Strg+Z nimmt seine Änderungen zurück, Esc bricht ab, die Zeilen, an denen er arbeitet, \
             sind farbig umrandet. Nur für Programme dieses Benutzers auf diesem Rechner erreichbar. \
             Aus: das Fenster antwortet nur mit einem Hinweis.",
        );
        pick
    }

    /// Zeilenabstand and Abstand nach Absatz of the texts: one setting for the
    /// editor and the printouts.
    fn text_spacing_ui(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        form_kit::section(ui, palette, strings::TEXT_SPACING_SECTION);
        crate::widgets::row(ui, |ui| {
            ui.label("Zeilenabstand");
            form_kit::choice(
                ui,
                palette,
                &proava_application::LineSpacing::ALL.map(|line| (line, line.key())),
                &mut self.text_spacing.line,
            );
        });
        crate::widgets::row(ui, |ui| {
            ui.label("Abstand nach Absatz");
            form_kit::choice(
                ui,
                palette,
                &proava_application::ParagraphGap::ALL.map(|gap| (gap, gap.label())),
                &mut self.text_spacing.gap,
            );
        });
        form_kit::hint(
            ui,
            palette,
            "Gilt für den Texteditor und für alle Ausdrucke, damit beide gleich aussehen. Vorgabe wie in \
             AVA-Ausdrucken üblich: einfach, kein Abstand – Leerzeilen entstehen durch leere Absätze im Text.",
        );
    }

    /// Schnellauswahl of „Bieterangabe einfügen“ / „Angabe des Auftraggebers einfügen“.
    fn complement_captions(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        form_kit::section(ui, palette, "Bieterangaben im Langtext");
        form_kit::hint(
            ui,
            palette,
            "Vortexte der Schnellauswahl (Langtext-Leiste, Rechtsklick, Strg+Umschalt+B bzw. \
             Strg+Umschalt+G) – je Zeile einer, z. B. „Fabrikat:“.",
        );
        ui.columns(2, |columns| {
            let (left, right) = columns.split_at_mut(1);
            for (ui, label, text, default) in [
                (
                    &mut left[0],
                    "Bieterangaben (vom Bieter)",
                    &mut self.bidder_captions,
                    proava_application::default_bidder_captions(),
                ),
                (
                    &mut right[0],
                    "Angaben des Auftraggebers",
                    &mut self.owner_captions,
                    proava_application::default_owner_captions(),
                ),
            ] {
                ui.label(
                    egui::RichText::new(label)
                        .size(12.0)
                        .color(palette.text_dim),
                );
                ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_rows(7)
                        .desired_width(f32::INFINITY),
                );
                if ui
                    .small_button("Vorgabe")
                    .on_hover_text("Liste von proAVA wiederherstellen")
                    .clicked()
                {
                    *text = default.join("\n");
                }
            }
        });
    }

    fn new_projects(&mut self, ui: &mut egui::Ui, palette: &Palette) -> Option<PickTarget> {
        let mut pick = None;
        form_kit::section(ui, palette, "Standardordner");
        if form_kit::path_field(
            ui,
            palette,
            "Ordner für neue Projekte (leer = zuletzt benutzter Ordner)",
            &mut self.default_folder,
            "z. B. /home/…/Projekte",
            strings::CHOOSE_FOLDER,
        ) {
            pick = Some(PickTarget::Folder);
        }
        ui.add_space(10.0);
        form_kit::section(ui, palette, "Erstes Leistungsverzeichnis");
        let mask_error = OzMask::parse(self.mask.trim()).err();
        form_kit::field(
            ui,
            palette,
            "OZ-Maske",
            &mut self.mask,
            mask_error.as_deref(),
        );
        if let Ok(mask) = OzMask::parse(self.mask.trim()) {
            form_kit::hint(
                ui,
                palette,
                &format!("Beispiel: {}", mask.example_with_step(self.step)),
            );
            ui.add_space(6.0);
        }
        ui.label(
            RichText::new(strings::WS_STEP_LABEL)
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(3.0);
        form_kit::choice(
            ui,
            palette,
            &[(1, "1"), (10, "10"), (100, "100"), (1000, "1000")],
            &mut self.step,
        );
        let vat_error = parse_decimal_de(self.vat.trim().trim_end_matches('%').trim())
            .err()
            .map(|_| "Keine Zahl.".to_string());
        form_kit::field(
            ui,
            palette,
            "Umsatzsteuer in %",
            &mut self.vat,
            vat_error.as_deref(),
        );
        ui.add_space(10.0);
        form_kit::section(ui, palette, "Positionsbibliothek");
        if form_kit::path_field(
            ui,
            palette,
            "Datei der eigenen Bibliothek (leer = proAVA-Bibliothek.proava im Home-Ordner)",
            &mut self.library,
            "z. B. /home/…/Bibliothek.proava",
            "Datei wählen …",
        ) {
            pick = Some(PickTarget::Library);
        }
        form_kit::hint(
            ui,
            palette,
            "Ein normales proAVA-Projekt; „In Bibliothek übernehmen …“ (Rechtsklick auf eine Zeile) legt es beim ersten Mal an.",
        );
        ui.add_space(8.0);
        ui.label(
            RichText::new("Weitere Bibliotheken (z. B. Büro auf dem Netzlaufwerk) – im Seitenfenster umschaltbar")
                .size(12.0)
                .color(palette.text_dim),
        );
        let mut remove = None;
        for (index, (path, label, read_only)) in self.libraries.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(label)
                        .hint_text("Name, z. B. Büro")
                        .desired_width(110.0),
                );
                ui.add(
                    egui::TextEdit::singleline(path)
                        .hint_text("/…/Buero-Bibliothek.proava")
                        .desired_width((ui.available_width() - 210.0).max(120.0)),
                );
                ui.checkbox(read_only, "schreibgeschützt");
                if ui.small_button("Entfernen").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            self.libraries.remove(index);
        }
        if ui.button("Weitere Bibliothek hinzufügen").clicked() {
            self.libraries.push((String::new(), String::new(), true));
        }
        ui.add_space(6.0);
        ui.checkbox(
            &mut self.library_own_window,
            "„Bibliothek bearbeiten …“ öffnet ein eigenes Fenster (das Projekt bleibt offen)",
        )
        .on_hover_text(
            "Aus: Die Bibliothek ersetzt das Projekt in diesem Fenster; „Zurück zu …“ oben öffnet das Projekt wieder.",
        );
        ui.add_space(10.0);
        form_kit::section(ui, palette, "Projektvorlagen");
        if form_kit::path_field(
            ui,
            palette,
            "Ordner der eigenen Vorlagen (leer = proAVA-Vorlagen im Home-Ordner)",
            &mut self.template_folder,
            "z. B. /home/…/proAVA-Vorlagen",
            strings::CHOOSE_FOLDER,
        ) {
            pick = Some(PickTarget::Templates);
        }
        form_kit::hint(
            ui,
            palette,
            "Jede Vorlage ist ein normales proAVA-Projekt; „Als Vorlage speichern …“ im Menü Projekt legt sie hier ab.",
        );
        ui.add_space(10.0);
        form_kit::section(
            ui,
            palette,
            "Textquellen (AUSSCHREIBEN.DE, STLB-Bau Online)",
        );
        ui.radio_value(
            &mut self.web_pages_in_system_browser,
            false,
            "Im Browserfenster von proAVA (exportierte GAEB-Dateien kommen direkt in den Übernahme-Dialog)",
        );
        ui.radio_value(
            &mut self.web_pages_in_system_browser,
            true,
            "Im System-Browser (proAVA beobachtet den Download-Ordner)",
        );
        let hint = match crate::jobs::helper_missing() {
            None => "Das Browserfenster zeigt die Seiten unverändert und merkt sich Anmeldung und Merkliste in einem eigenen Profil.".to_string(),
            Some(reason) => format!("Es gilt der System-Browser: {reason}"),
        };
        form_kit::hint(ui, palette, &hint);
        pick
    }

    fn saving(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        form_kit::section(ui, palette, "Wiederherstellungsdatei");
        ui.checkbox(
            &mut self.autosave,
            "Ungespeicherte Änderungen automatisch sichern",
        );
        ui.add_enabled_ui(self.autosave, |ui| {
            crate::widgets::row(ui, |ui| {
                ui.label("alle");
                ui.add(egui::DragValue::new(&mut self.autosave_minutes).range(1..=120));
                ui.label("Minuten");
            });
        });
        form_kit::hint(
            ui,
            palette,
            "Die Sicherung liegt versteckt neben dem Projekt; nach einem Absturz bietet proAVA sie an.",
        );
        ui.add_space(10.0);
        form_kit::section(ui, palette, "Beim Schließen");
        ui.checkbox(
            &mut self.backup_on_close,
            "Beim Schließen eines Projekts eine Sicherung (Zip) anlegen",
        );
    }

    fn comparison(&mut self, ui: &mut egui::Ui, palette: &Palette) {
        form_kit::section(ui, palette, "Vorgaben für neue Preisspiegel-Ansichten");
        crate::widgets::row(ui, |ui| {
            ui.label("Ausreißer ab");
            ui.add(
                egui::DragValue::new(&mut self.outlier)
                    .range(1..=100)
                    .suffix(" %"),
            );
            ui.label("Abweichung");
        });
        ui.add_space(6.0);
        crate::widgets::row(ui, |ui| {
            ui.label("Bezugsgröße");
            egui::ComboBox::from_id_salt("settings_reference")
                .selected_text(self.reference.label())
                .show_ui(ui, |ui| {
                    for reference in [
                        DeviationReference::Mean,
                        DeviationReference::Lowest,
                        DeviationReference::Estimate,
                    ] {
                        ui.selectable_value(&mut self.reference, reference, reference.label());
                    }
                });
        });
        ui.add_space(6.0);
        crate::widgets::row(ui, |ui| {
            ui.label("Aufgreifschwelle");
            ui.add(
                egui::DragValue::new(&mut self.undercut)
                    .range(1..=50)
                    .suffix(" %"),
            );
        });
        form_kit::hint(
            ui,
            palette,
            "§ 16d Abs. 1 VOB/A: Hinweis, wenn der Günstigste so weit unter dem Zweiten liegt.",
        );
    }

    fn print(&mut self, ui: &mut egui::Ui, palette: &Palette) -> Option<PickTarget> {
        let mut pick = None;
        form_kit::section(ui, palette, "Aufsteller (Planer) für neue Projekte");
        form_kit::address(ui, palette, &mut self.planner);
        crate::widgets::row(ui, |ui| {
            ui.label("Bearbeiter");
            ui.add(
                egui::TextEdit::singleline(&mut self.print_author)
                    .hint_text("z. B. Vorname Name")
                    .desired_width(260.0),
            );
        });
        form_kit::hint(
            ui,
            palette,
            "Für {ersteller} und {bearbeiter} in Kopf- und Fußzeile und für den Titelblock; leer = Name des Planers.",
        );
        form_kit::section(ui, palette, "Logo in der Kopfzeile");
        if form_kit::path_field(
            ui,
            palette,
            "Bilddatei (optional)",
            &mut self.logo,
            "PNG oder JPEG",
            "Datei wählen …",
        ) {
            pick = Some(PickTarget::Logo);
        }
        if !self.logo.is_empty() && ui.button("Kein Logo").clicked() {
            self.logo.clear();
        }
        ui.add_space(10.0);
        form_kit::hint(
            ui,
            palette,
            "Zeilenabstand und Abstand nach Absatz der Texte: unter Darstellung (gilt für Editor und Druck).",
        );
        form_kit::section(ui, palette, "Druckstil");
        ui.horizontal_wrapped(|ui| {
            for style in proava_application::PrintStyle::ALL {
                ui.radio_value(&mut self.print_style, style, style.label());
            }
        });
        form_kit::hint(
            ui,
            palette,
            "Klassisch: schwarze Schrift, Titelzeilen in der Akzentfarbe hinterlegt. proAVA: Überschriften \
             in der Akzentfarbe. Vorgabe für alle Dokumentarten; im Druckdialog je Dokumentart änderbar.",
        );
        crate::widgets::row(ui, |ui| {
            ui.label("Akzentfarbe");
            let parsed = proava_application::PrintLook::parse_accent(&self.print_accent);
            let mut rgb = parsed
                .as_deref()
                .and_then(|hex| u32::from_str_radix(&hex[1..], 16).ok())
                .map(|value| [(value >> 16) as u8, (value >> 8) as u8, value as u8])
                .unwrap_or([0, 79, 124]);
            if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                self.print_accent = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
            }
            ui.add(egui::TextEdit::singleline(&mut self.print_accent).desired_width(80.0));
            if parsed.is_none() {
                ui.label(RichText::new("#RRGGBB").color(palette.text_dim));
            }
            if ui
                .button("Petrolblau")
                .on_hover_text("#004F7C (RGB 0/79/124)")
                .clicked()
            {
                self.print_accent = proava_application::PrintLook::DEFAULT_ACCENT.to_string();
            }
        });
        form_kit::hint(
            ui,
            palette,
            "Linie unter dem Tabellenkopf und Flächen (Titel- und Summenzeilen) aller Ausdrucke.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.print_border, "Blattrahmen");
            ui.add_enabled_ui(self.print_border, |ui| {
                ui.label(RichText::new("Strich").size(12.5).color(palette.text_dim));
                for width in proava_application::BorderWidth::ALL {
                    ui.radio_value(&mut self.print_border_width, width, width.label());
                }
            });
        });
        form_kit::hint(
            ui,
            palette,
            "Linie um Kopfzeile, Inhalt und Fußzeile jeder Seite, auch um das Deckblatt; Briefe ohne. \
             Vorgabe für alle Dokumentarten, im Druckdialog je Dokumentart änderbar.",
        );
        ui.checkbox(
            &mut self.print_table_grid,
            "Tabellen voll umrandet (Gitter)",
        );
        form_kit::hint(
            ui,
            palette,
            "Alle Tabellen-Ausdrucke mit Außenrahmen und allen senkrechten und waagrechten Linien; im LV ein \
             Rahmen je Position. Vorgabe für alle Dokumentarten außer dem Preisspiegel (ab Werk umrandet), im \
             Druckdialog je Dokumentart änderbar.",
        );
        ui.add_space(10.0);
        form_kit::section(ui, palette, "Deckblatt");
        ui.label(
            RichText::new(
                "Form (Standard, Briefkopf, Tabelle 1, Tabelle 2 oder ohne) wählt man im Druckdialog je Dokumentart.",
            )
            .size(12.0)
            .color(palette.text_dim),
        );
        ui.checkbox(
            &mut self.cover_priced,
            "Bepreistes LV standardmäßig mit Deckblatt (die Angebotsaufforderung hat es von Haus aus)",
        );
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("Auf dem Deckblatt:")
                    .size(12.5)
                    .color(palette.text_dim),
            );
            ui.checkbox(&mut self.cover_blocks.logo, "Logo");
            ui.checkbox(&mut self.cover_blocks.addresses, "Adressen");
            ui.checkbox(&mut self.cover_blocks.facts, "Kenndaten");
            ui.checkbox(
                &mut self.cover_blocks.bidder,
                "Bieterfeld (Angebotsaufforderung)",
            );
        });
        ui.add_space(10.0);
        form_kit::section(ui, palette, "GAEB-Export");
        ui.checkbox(
            &mut self.gaeb_images,
            "Bilder der Langtexte mitgeben (GAEB DA XML; GAEB 90 kann keine Bilder)",
        );
        ui.checkbox(
            &mut self.csv_windows_1252,
            "CSV-Export in Windows-1252 statt UTF-8 (für ältere Programme)",
        );
        pick
    }
}

/// Where a chooser opens: the folder of the field's path, else the folder
/// last used in a file dialog, else the folder for new projects, else home.
fn start_folder(launcher: &Launcher, text: &str, file: bool) -> PathBuf {
    let path = PathBuf::from(text.trim());
    let folder = if text.trim().is_empty() {
        None
    } else if file {
        path.parent().map(Path::to_path_buf)
    } else {
        Some(path)
    };
    folder
        .filter(|folder| launcher.is_folder(folder))
        .or_else(|| launcher.recent_file_folders().into_iter().next())
        .or_else(|| launcher.suggested_folder())
        .unwrap_or_else(|| launcher.home_folder())
}

fn vat_text(vat: proava_domain::Decimal) -> String {
    format_decimal_de(vat, 2)
        .trim_end_matches('0')
        .trim_end_matches(',')
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use proava_application::{PanelSide, Preferences, ThemeChoice};
    use proava_domain::{Address, Decimal, DeviationReference};

    use super::{SettingsDialog, SettingsTab};

    fn dialog() -> SettingsDialog {
        let preferences = Preferences::default();
        SettingsDialog {
            tab: SettingsTab::Appearance,
            theme: ThemeChoice::Light,
            zoom_percent: 100,
            side: PanelSide::Left,
            detail_visible: true,
            reset_width: false,
            mark_empty: true,
            clip_previews_hidden: false,
            windows_in_front: true,
            title_rows_filled: true,
            default_folder: String::new(),
            mask: preferences.new_lv_mask.clone(),
            step: preferences.new_lv_step,
            vat: "19".into(),
            autosave: true,
            autosave_minutes: 5,
            backup_on_close: false,
            outlier: 20,
            reference: DeviationReference::Mean,
            undercut: 10,
            planner: Address::default(),
            logo: String::new(),
            cover_priced: false,
            cover_blocks: proava_application::CoverBlocks::default(),
            print_style: proava_application::PrintStyle::default(),
            print_border: false,
            print_border_width: proava_application::BorderWidth::default(),
            print_table_grid: false,
            print_accent: proava_application::PrintLook::DEFAULT_ACCENT.to_string(),
            text_spacing: proava_application::TextSpacing::default(),
            print_author: String::new(),
            page_frame: proava_domain::PageFrame::default(),
            gaeb_images: true,
            csv_windows_1252: false,
            web_pages_in_system_browser: false,
            ai_windows: true,
            library: String::new(),
            libraries: Vec::new(),
            library_own_window: true,
            template_folder: String::new(),
            spell_check: true,
            dictionary_folder: String::new(),
            bidder_captions: preferences.bidder_captions.join("\n"),
            owner_captions: preferences.owner_captions.join("\n"),
            errors: Vec::new(),
            picker: None,
        }
    }

    #[test]
    fn caption_lists_are_one_per_line() {
        let mut form = dialog();
        form.bidder_captions =
            "Fabrikat:\n\n  Typ:  \nFabrikat:\nRaumbediengerät, Fabrikat:".into();
        let (preferences, _) = form.to_preferences(|_| true, |_| true).unwrap();
        assert_eq!(
            preferences.bidder_captions,
            ["Fabrikat:", "Typ:", "Raumbediengerät, Fabrikat:"].map(str::to_string)
        );
        assert_eq!(
            preferences.owner_captions,
            proava_application::default_owner_captions()
        );
    }

    #[test]
    fn form_turns_into_preferences() {
        let mut form = dialog();
        form.zoom_percent = 125;
        form.vat = "7,5 %".into();
        form.planner.name1 = "  Planungsbüro Muster ".into();
        form.default_folder = "/projekte".into();
        form.logo = "/bilder/logo.PNG".into();
        let (preferences, folder) = form.to_preferences(|_| true, |_| true).unwrap();
        assert_eq!(preferences.ui_zoom, 1.25);
        assert_eq!(preferences.new_lv_vat, "7.5".parse::<Decimal>().unwrap());
        assert_eq!(preferences.default_planner.name1, "Planungsbüro Muster");
        assert_eq!(folder.as_deref(), Some(Path::new("/projekte")));
        assert!(preferences.logo_file.is_some());
        let mut form = dialog();
        form.page_frame.footer[2] = "S. {seite}\nzweite\ndritte".into();
        form.page_frame.font_size_pt = 3;
        let (framed, _) = form.to_preferences(|_| true, |_| true).unwrap();
        assert_eq!(
            framed.page_frame.footer[2], "S. {seite}\nzweite",
            "höchstens zwei Zeilen"
        );
        assert_eq!(framed.page_frame.font_size_pt, 7);
    }

    #[test]
    fn wrong_entries_are_reported() {
        let mut form = dialog();
        form.vat = "neunzehn".into();
        form.mask = "?".into();
        form.default_folder = "/gibtsnicht".into();
        form.logo = "/bilder/logo.gif".into();
        let problems = form.to_preferences(|_| false, |_| true).unwrap_err();
        assert_eq!(problems.len(), 4, "{problems:?}");
    }
}
