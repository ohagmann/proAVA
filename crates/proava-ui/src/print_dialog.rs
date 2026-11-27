//! Drucken … (Strg+P): document kind, options, range; the shell remembers
//! the options per kind, writes the PDF in the background and opens it.
//! Every PDF entry of the menus (Drucken ▸ LV ohne Preise … and so on)
//! opens this dialog with its kind chosen; Enter prints right away.

use eframe::egui::{self, RichText};
use proava_application::{ComparisonPrint, PrintJob, PrintKind, PrintSettings};
use proava_domain::{BidId, CostEstimateId, InvoiceId, Project, RowId};

use crate::theme::Palette;
use crate::widgets;

pub struct PrintDialog {
    kind: PrintKind,
    settings: PrintSettings,
    bid: Option<BidId>,
    /// Invoice of the Abrechnung (Aufmaßprüfung, Rechnungsfreigabe).
    invoice: Option<InvoiceId>,
    /// Kostenschätzung / Kostenberechnung to print.
    cost_estimate: Option<CostEstimateId>,
    /// Kostenverfolgung: the view of the tab.
    tracking: proava_domain::TrackOptions,
    /// Titles of the selection in the LV (empty = none marked).
    marked: Vec<RowId>,
    marked_label: String,
    only_marked: bool,
    /// Open the PDF after it is written.
    pub open_after: bool,
    pages: Option<Result<usize, String>>,
    counting: bool,
    /// Druckstil from the settings, shown while the kind has none of its own.
    pub default_style: proava_application::PrintStyle,
    /// Blattrahmen from the settings, shown while the kind has no choice of its own.
    pub default_border: bool,
    /// Tabellen voll umrandet (Gitter) from the settings, likewise.
    pub default_grid: bool,
}

pub enum PrintDialogResult {
    Open,
    Cancelled,
    /// The kind changed from the first to the second: remember the options
    /// of the old one and load those of the new one ([`PrintDialog::load`]).
    KindChanged(PrintKind, PrintKind),
    /// Count the pages of this job in the background ([`PrintDialog::pages`]).
    CountPages(PrintJob),
    /// Pick a file and write this job.
    Print(PrintJob),
    /// Close and open Projektstammdaten ▸ Kopf-/Fußzeile.
    EditFrame,
}

impl PrintDialog {
    /// `marked` = titles of the LV selection with a label for the dialog.
    pub fn new(
        kind: PrintKind,
        settings: PrintSettings,
        project: &Project,
        marked: Vec<RowId>,
    ) -> Self {
        let marked_label = marked
            .iter()
            .filter_map(|id| project.lv().row_by_id(*id))
            .map(|row| format!("{} {}", row.ordinal(), row.short_text()))
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            kind,
            settings,
            bid: project
                .awarded_bid()
                .or_else(|| project.bids().first())
                .map(|bid| bid.id()),
            invoice: project
                .billing()
                .invoices()
                .last()
                .map(|invoice| invoice.id),
            cost_estimate: project.cost_estimates().last().map(|estimate| estimate.id),
            tracking: proava_domain::TrackOptions::default(),
            only_marked: false,
            marked,
            marked_label,
            open_after: true,
            pages: None,
            counting: false,
            default_style: proava_application::PrintStyle::default(),
            default_border: false,
            default_grid: false,
        }
    }

    pub fn kind(&self) -> PrintKind {
        self.kind
    }

    /// Preselects the bid (Auftrags-LV as PDF from the Angebote).
    pub fn set_bid(&mut self, bid: BidId) {
        self.bid = Some(bid);
    }

    /// Preselects the Kostenermittlung (from the Reiter Kosten).
    pub fn set_cost_estimate(&mut self, estimate: Option<CostEstimateId>) {
        if estimate.is_some() {
            self.cost_estimate = estimate;
        }
    }

    /// The view of the Reiter Kostenverfolgung for its printout.
    pub fn set_tracking(&mut self, options: proava_domain::TrackOptions) {
        self.tracking = options;
    }

    /// Preselects the invoice (from the Reiter Abrechnung).
    pub fn set_invoice(&mut self, invoice: Option<InvoiceId>) {
        if invoice.is_some() {
            self.invoice = invoice;
        }
    }

    pub fn settings(&self) -> &PrintSettings {
        &self.settings
    }

    /// Options of the newly chosen kind.
    pub fn load(&mut self, settings: PrintSettings) {
        self.settings = settings;
        self.pages = None;
    }

    /// Result of a page count started with [`PrintDialogResult::CountPages`].
    pub fn pages(&mut self, result: Result<usize, String>) {
        self.pages = Some(result);
        self.counting = false;
    }

    fn job(&self, comparison: &ComparisonPrint) -> PrintJob {
        PrintJob {
            kind: self.kind,
            settings: self.settings.clone(),
            bid: self.bid.filter(|_| self.kind.needs_bid()),
            titles: if self.only_marked && self.kind.is_lv() {
                self.marked.clone()
            } else {
                Vec::new()
            },
            comparison: comparison.clone(),
            logo: None,
            frame: proava_domain::PageFrame::default(),
            file_name: String::new(),
            cover_blocks: proava_application::CoverBlocks::default(),
            invoice: self.invoice.filter(|_| self.kind.needs_invoice()),
            cost_estimate: self
                .cost_estimate
                .filter(|_| self.kind.needs_cost_estimate()),
            look: proava_application::PrintLook::default(),
            author: String::new(),
            tracking: self.tracking.clone(),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        comparison: &ComparisonPrint,
    ) -> PrintDialogResult {
        let before = (
            self.kind,
            self.settings.clone(),
            self.bid,
            self.only_marked,
            self.invoice,
            self.cost_estimate,
        );
        let mut count = false;
        let mut edit_frame = false;
        // Enter prints – unless it ends typing in a text field of the dialog.
        let typing = ui.ctx().egui_wants_keyboard_input();
        let enter = !typing && ui.input(|input| input.key_pressed(egui::Key::Enter));
        let max_height = (ui.ctx().content_rect().height() - 300.0).max(220.0);
        let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 660.0, |ui| {
            widgets::title(
                ui,
                palette,
                "Drucken",
                "Dokumentart und Optionen wählen; die Optionen merkt sich proAVA je Dokumentart. \
                 Enter speichert das PDF.",
            );
            egui::ScrollArea::vertical()
                .id_salt("print_dialog_body")
                .max_height(max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
            ui.columns(2, |columns| {
                let ui = &mut columns[0];
                heading(ui, palette, "Dokumentart");
                let kind_before = self.kind;
                for kind in PrintKind::ALL {
                    if kind == PrintKind::PriceComparison || kind == PrintKind::BillingCheck {
                        ui.add_space(4.0);
                    }
                    let enabled = available(project, kind) || kind == kind_before;
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.radio_value(&mut self.kind, kind, kind.label());
                    });
                    if kind == kind_before {
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(kind.description()).size(11.5).color(palette.text_dim),
                                )
                                .wrap(),
                            );
                        });
                        ui.add_space(2.0);
                    }
                }
                if self.kind.needs_bid() {
                    ui.add_space(6.0);
                    let name = |id: Option<BidId>| {
                        project
                            .bids()
                            .iter()
                            .find(|bid| Some(bid.id()) == id)
                            .map(|bid| bid.name().to_string())
                            .unwrap_or_else(|| "Angebot wählen".to_string())
                    };
                    crate::widgets::row(ui, |ui| {
                        ui.label(RichText::new("Bieter").size(12.5).color(palette.text_dim));
                        egui::ComboBox::from_id_salt("print_bid")
                            .selected_text(name(self.bid))
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                for bid in project.bids() {
                                    let awarded = project.awarded_bid().map(|awarded| awarded.id()) == Some(bid.id());
                                    let text = if awarded {
                                        format!("{} (beauftragt)", bid.name())
                                    } else {
                                        bid.name().to_string()
                                    };
                                    ui.selectable_value(&mut self.bid, Some(bid.id()), text);
                                }
                            });
                    });
                }
                if self.kind.needs_invoice() {
                    ui.add_space(6.0);
                    let invoices = project.billing().invoices();
                    let name = |id: Option<InvoiceId>| {
                        invoices
                            .iter()
                            .enumerate()
                            .find(|(_, invoice)| Some(invoice.id) == id)
                            .map(|(index, invoice)| invoice_label(index, invoice))
                            .unwrap_or_else(|| "Rechnung wählen".to_string())
                    };
                    egui::ComboBox::from_id_salt("print_invoice")
                        .selected_text(name(self.invoice))
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for (index, invoice) in invoices.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.invoice,
                                    Some(invoice.id),
                                    invoice_label(index, invoice),
                                );
                            }
                        });
                }

                if self.kind.needs_cost_estimate() {
                    ui.add_space(6.0);
                    let name = |id: Option<CostEstimateId>| {
                        id.and_then(|id| project.cost_estimate(id))
                            .map(|estimate| estimate.display_name())
                            .unwrap_or_else(|| "Kostenermittlung wählen".to_string())
                    };
                    egui::ComboBox::from_id_salt("print_cost_estimate")
                        .selected_text(name(self.cost_estimate))
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for estimate in project.cost_estimates() {
                                ui.selectable_value(&mut self.cost_estimate, Some(estimate.id), estimate.display_name());
                            }
                        });
                }

                let ui = &mut columns[1];
                // Druckstil first and large: it applies to every kind.
                heading(ui, palette, "Stil");
                ui.horizontal(|ui| {
                    let current = self.settings.style.unwrap_or(self.default_style);
                    for style in proava_application::PrintStyle::ALL {
                        // Both with a frame: two clear choices, the chosen one filled.
                        let button = egui::Button::selectable(
                            current == style,
                            RichText::new(style.label()).size(13.5),
                        )
                        .frame_when_inactive(true)
                        .min_size(egui::vec2(110.0, 30.0));
                        let hint = match style {
                            proava_application::PrintStyle::Classic => {
                                "Schwarze Schrift, Titelzeilen in der Akzentfarbe hinterlegt"
                            }
                            proava_application::PrintStyle::ProAva => {
                                "Überschriften und Spaltenköpfe in der Akzentfarbe"
                            }
                        };
                        if ui.add(button).on_hover_text(hint).clicked() {
                            self.settings.style = Some(style);
                        }
                    }
                });
                ui.label(
                    RichText::new("Vorgabe und Akzentfarbe: Extras › Einstellungen › Drucken")
                        .size(11.0)
                        .color(palette.text_dim),
                );
                ui.add_space(8.0);
                if self.kind.needs_cost_estimate() {
                    let settings = &mut self.settings;
                    heading(ui, palette, "Inhalt");
                    ui.checkbox(&mut settings.cover, "Deckblatt mit den Kennzahlen");
                    ui.checkbox(&mut settings.chart, "Balkendiagramm der Anteile")
                        .on_hover_text("Kostenberechnung: Anteile der KG 481–489; Kostenschätzung: Anteile der Bauteile");
                    ui.checkbox(&mut settings.plants, "Anlagenliste je Bauteil");
                    ui.checkbox(&mut settings.cost_control, "Kostenkontrolle gegen die LVs und den Auftrag")
                        .on_hover_text("Nur wenn ein LV Preise oder einen Auftrag hat");
                    ui.add_space(6.0);
                    heading(ui, palette, "Seite");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut settings.landscape, true, "A4 quer");
                        ui.radio_value(&mut settings.landscape, false, "A4 hoch");
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Zusammenfassung aller Bauteile, je Bauteil Datenpunkte, Aufteilung auf die \
                             Kostengruppen mit Prüfsumme, Erläuterungen mit Kennwerten und Annahmen.",
                        )
                        .size(11.5)
                        .color(palette.text_dim),
                    );
                } else if self.kind == PrintKind::CostTracking {
                    let settings = &mut self.settings;
                    heading(ui, palette, "Inhalt");
                    ui.checkbox(&mut settings.cover, "Deckblatt mit den Kennzahlen");
                    ui.checkbox(&mut settings.chart, "Diagramm der Stufen je Kostengruppe")
                        .on_hover_text("Kostenvorgabe, Kostenermittlungen, Angebot, Auftrag, Prognose und Abrechnung als Balken");
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "A4 quer. Tabelle wie im Reiter: Ebene {}, {}, {}, Abweichung {}. \
                             Danach Erläuterungen und Maßnahmen, Grundlagen und Annahmen.",
                            self.tracking.level,
                            match self.tracking.view {
                                proava_domain::TrackView::ByGroup => "nach Kostengruppen",
                                proava_domain::TrackView::ByLv => "nach Vergabeeinheiten",
                            },
                            if self.tracking.gross { "brutto" } else { "netto" },
                            self.tracking.deviation.label(),
                        ))
                        .size(11.5)
                        .color(palette.text_dim),
                    );
                } else if self.kind == PrintKind::CostLv {
                    let settings = &mut self.settings;
                    heading(ui, palette, "Inhalt");
                    ui.checkbox(&mut settings.notes, "Angaben und Vorbemerkungen")
                        .on_hover_text("Kostenstand, Grundlagen, Verfahren und Quellen der Kennwerte (DIN 276, 4.2)");
                    ui.checkbox(&mut settings.summary, "Kurztext-Liste der Positionen");
                    ui.add_enabled_ui(settings.summary, |ui| {
                        ui.checkbox(&mut settings.long_texts, "Langtexte (z. B. Quelle des Kennwerts)");
                    });
                    ui.add_space(6.0);
                    heading(ui, palette, "Seite");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut settings.landscape, false, "A4 hoch");
                        ui.radio_value(&mut settings.landscape, true, "A4 quer");
                    });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            "Das aktive Kosten-LV: Zusammenstellung nach Kostengruppen in den Ebenen der \
                             Stufe (Kostenschätzung bis zur 2., Kostenberechnung bis zur 3. Ebene) mit \
                             Summe netto, Umsatzsteuer und brutto, danach die Kurztext-Liste.",
                        )
                        .size(11.5)
                        .color(palette.text_dim),
                    );
                } else if self.kind.is_lv() {
                    let settings = &mut self.settings;
                    if self.kind == PrintKind::ShortTextLv {
                        heading(ui, palette, "Preise");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut settings.prices, true, "mit Preisen");
                            ui.radio_value(&mut settings.prices, false, "ohne (EP/GP zum Ausfüllen)");
                        });
                        ui.add_space(4.0);
                    }
                    heading(ui, palette, "Inhalt");
                    ui.checkbox(&mut settings.long_texts, "Langtexte");
                    ui.add_enabled_ui(settings.long_texts, |ui| {
                        ui.checkbox(&mut settings.images, "Bilder");
                    });
                    ui.checkbox(&mut settings.notes, "Hinweise und Vorbemerkungen");
                    ui.checkbox(&mut settings.performance, "Ausführungsbeschreibungen");
                    ui.add_enabled_ui(self.kind != PrintKind::Summary, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut settings.cost_groups, "Kostengruppe")
                                .on_hover_text("An jeder Position rechts neben dem Kurztext, z. B. „KG 481“ (eigene oder vom Titel)");
                            ui.checkbox(&mut settings.work_categories, "Leistungsbereich")
                                .on_hover_text("An jeder Position rechts neben dem Kurztext, z. B. „LB 070“ (eigener oder vom Titel)");
                        });
                    });
                    if self.kind.offers_takeoff() {
                        ui.checkbox(&mut settings.takeoff, "Mengenermittlung")
                            .on_hover_text("Unter jeder Position die Zeilen der Mengenermittlung mit Summe");
                    }
                    ui.add_enabled_ui(self.kind != PrintKind::Summary, |ui| {
                        ui.checkbox(&mut settings.summary, "Zusammenstellung am Ende");
                    });
                    // Column „Anteil“: only where the Zusammenstellung has sums.
                    ui.add_enabled_ui(
                        (settings.summary || self.kind == PrintKind::Summary)
                            && self.kind != PrintKind::RequestForQuotation,
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.add_space(18.0);
                                ui.checkbox(&mut settings.summary_share, "mit Anteil der Titelsummen in %")
                                    .on_hover_text("Spalte „Anteil“: jede Titelsumme in Prozent der Summe netto");
                            });
                        },
                    );
                    ui.add_enabled_ui(self.kind != PrintKind::Summary, |ui| {
                        ui.checkbox(&mut settings.title_page_break, "Neuer Titel auf neuer Seite");
                        ui.checkbox(&mut settings.contents, "Inhaltsverzeichnis mit Seitenzahlen");
                        ui.checkbox(&mut settings.carry, "Übertrag am Seitenende und -anfang");
                        ui.checkbox(&mut settings.bookmark_positions, "Lesezeichen für Positionen")
                            .on_hover_text(
                                "PDF-Lesezeichen auch für jede Position unter ihrem Titel; \
                                 Titel, Deckblatt, Hinweise und Zusammenstellung haben immer eins",
                            );
                    });
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Deckblatt").size(12.5).strong().color(palette.text));
                        for form in proava_application::CoverForm::ALL {
                            if ui.radio(settings.cover && settings.cover_form == form, form.label()).clicked() {
                                settings.cover = true;
                                settings.cover_form = form;
                            }
                        }
                        if ui.radio(!settings.cover, "ohne").clicked() {
                            settings.cover = false;
                        }
                    })
                    .response
                    .on_hover_text(
                        "Standard: klar gegliedert, Bieterfeld unten. Briefkopf: Vergabestelle oben links, \
                         Kenndaten im Kasten. Tabelle 1: Adressen und Kenndaten in umrandeten Tabellen. \
                         Tabelle 2: wie Tabelle 1, jedes Adressfeld in einer eigenen Zeile. \
                         Die Vertragsbedingungen folgen auf einer eigenen Seite.",
                    );
                    ui.add_space(6.0);
                    heading(ui, palette, "Bereich");
                    ui.radio_value(&mut self.only_marked, false, "ganzes LV");
                    ui.add_enabled_ui(!self.marked.is_empty(), |ui| {
                        let label = if self.marked.is_empty() {
                            "markierte Titel (im LV Titel markieren)".to_string()
                        } else {
                            format!("markierte Titel: {}", shorten(&self.marked_label, 50))
                        };
                        ui.radio_value(&mut self.only_marked, true, label);
                    });
                    ui.add_space(6.0);
                    heading(ui, palette, "Seite");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut settings.landscape, false, "A4 hoch");
                        ui.radio_value(&mut settings.landscape, true, "A4 quer");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Schrift");
                        for size in [9u8, 10, 11] {
                            ui.radio_value(&mut settings.font_size_pt, size, format!("{size} pt"));
                        }
                    });
                } else {
                    let hint = match self.kind {
                        PrintKind::Submission => {
                            "Niederschrift über die Angebotsöffnung (A4 quer) für das gewählte LV: \
                             Vergabeangaben aus den Projektstammdaten, je Bieter aus dem \
                             Bieterverzeichnis Eingang, Status, Summen mit Nachlass, \
                             Nebenangebote und Bemerkungen; darunter die Unterschriftsfelder."
                        }
                        PrintKind::CostGroups => {
                            "Kosten des gewählten LV nach Kostengruppen (DIN 276): Planersummen \
                             und die Summen der Bieter; ausgeschlossene Angebote fehlen."
                        }
                        PrintKind::BillingCheck => {
                            "Aufmaßprüfung der gewählten Rechnung (A4 quer): je Position Auftrag, \
                             bisher geprüft, Menge und Betrag der Rechnung gegen die geprüften, \
                             Quelle (Aufmaß, Fortschritt …) und Vermerke; Summen mit Nachlass und USt."
                        }
                        PrintKind::Release => {
                            "Rechnungsfreigabe der gewählten Rechnung (A4 hoch): Auftrag und Stand, \
                             Rechenweg mit Abzügen, Sicherheitseinbehalt und bisherigen Freigaben, \
                             Freigabebetrag; Unterschriftsfelder für Planer und Bauherr."
                        }
                        _ => {
                            "Der Preisspiegel wird so gedruckt, wie er im Reiter Preisspiegel \
                             eingestellt ist (Reihenfolge, anonym, Varianten)."
                        }
                    };
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.label(RichText::new(hint).size(12.0).color(palette.text_dim));
                    });
                }
            });
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(if project.info().page_frame.is_some() {
                        "Kopf-/Fußzeile: eigene des Projekts"
                    } else {
                        "Kopf-/Fußzeile: aus den Einstellungen"
                    })
                    .size(11.5)
                    .color(palette.text_dim),
                );
                if ui
                    .button("Kopf-/Fußzeile …")
                    .on_hover_text("Öffnet die Projektstammdaten, Reiter Kopf-/Fußzeile")
                    .clicked()
                {
                    edit_frame = true;
                }
                ui.add_space(12.0);
                let mut border = self.settings.border.unwrap_or(self.default_border);
                if ui
                    .checkbox(&mut border, "Blattrahmen")
                    .on_hover_text(
                        "Linie um Kopfzeile, Inhalt und Fußzeile jeder Seite; je Dokumentart gemerkt. \
                         Vorgabe und Strichstärke unter Extras › Einstellungen › Drucken.",
                    )
                    .changed()
                {
                    self.settings.border = Some(border);
                }
                if self.kind.offers_grid() {
                    ui.add_space(12.0);
                    let mut grid = self.settings.grid.unwrap_or(self.default_grid);
                    if ui
                        .checkbox(&mut grid, "Gitter")
                        .on_hover_text(
                            "Tabellen voll umrandet: Außenrahmen, alle senkrechten und waagrechten Linien; im LV ein Rahmen \
                         je Position und die Preiszeile in Feldern. Je Dokumentart gemerkt, Vorgabe unter Extras › \
                         Einstellungen › Drucken (der Preisspiegel ist ab Werk umrandet).",
                        )
                        .changed()
                    {
                        self.settings.grid = Some(grid);
                    }
                }
            });
            ui.label(
                RichText::new("Zusatzzeile Kopf – {kopftext}, dieser Ausdruck")
                    .size(11.0)
                    .color(palette.text_dim),
            );
            ui.add_sized(
                [ui.available_width(), 26.0],
                egui::TextEdit::singleline(&mut self.settings.header_text)
                    .hint_text("z. B. Vorabzug – nicht zur Ausführung"),
            );
            ui.label(
                RichText::new("Zusatzzeile Fuß – {fusstext}, dieser Ausdruck")
                    .size(11.0)
                    .color(palette.text_dim),
            );
            ui.add_sized(
                [ui.available_width(), 26.0],
                egui::TextEdit::singleline(&mut self.settings.footer_text),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.counting, egui::Button::new("Seiten zählen"))
                    .clicked()
                {
                    count = true;
                }
                let text = match (&self.pages, self.counting) {
                    (_, true) => "wird gezählt …".to_string(),
                    (Some(Ok(pages)), _) => crate::format::count(*pages, "Seite", "Seiten"),
                    (Some(Err(error)), _) => error.clone(),
                    (None, _) => String::new(),
                };
                ui.label(RichText::new(text).size(12.0).color(palette.text_dim));
                ui.add_space(12.0);
                ui.checkbox(&mut self.open_after, "nach dem Speichern öffnen");
            });
            ui.add_space(10.0);
            widgets::dialog_buttons(ui, palette, "PDF speichern …")
        });

        if (
            self.kind,
            self.settings.clone(),
            self.bid,
            self.only_marked,
            self.invoice,
            self.cost_estimate,
        ) != before
        {
            self.pages = None;
        }
        if self.kind != before.0 {
            return PrintDialogResult::KindChanged(before.0, self.kind);
        }
        if cancelled || close {
            return PrintDialogResult::Cancelled;
        }
        if edit_frame {
            return PrintDialogResult::EditFrame;
        }
        if self.kind.needs_bid() && self.kind == PrintKind::BidPrices && self.bid.is_none() {
            return PrintDialogResult::Open;
        }
        if self.kind.needs_cost_estimate() && self.cost_estimate.is_none() {
            return PrintDialogResult::Open;
        }
        if self.kind.needs_invoice() && self.invoice.is_none() {
            return PrintDialogResult::Open;
        }
        if count {
            self.counting = true;
            return PrintDialogResult::CountPages(self.job(comparison));
        }
        if confirmed || enter {
            return PrintDialogResult::Print(self.job(comparison));
        }
        PrintDialogResult::Open
    }
}

/// Section heading of the dialog (Dokumentart, Stil, Inhalt …).
fn heading(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).size(13.0).strong().color(palette.text));
    ui.add_space(2.0);
}

/// Whether the project has what `kind` prints (bids, an award, invoices …);
/// the print dialog and the menu Drucken grey out the others.
pub fn available(project: &Project, kind: PrintKind) -> bool {
    let tender = project.lv().purpose().is_tender();
    if !tender && crate::workspace::tender_print(kind) {
        return false;
    }
    match kind {
        PrintKind::CostLv => !tender,
        PrintKind::PriceComparison | PrintKind::BidPrices => !project.bids().is_empty(),
        PrintKind::Contract => project.awarded_bid().is_some(),
        PrintKind::Submission => !project.invitees().is_empty() || !project.bids().is_empty(),
        PrintKind::BillingCheck | PrintKind::Release => {
            project.contract().is_some() && !project.billing().invoices().is_empty()
        }
        PrintKind::CostEstimate => !project.cost_estimates().is_empty(),
        PrintKind::CostTracking => crate::tracking_view::has_tracking(project),
        _ => true,
    }
}

/// „AR-3 – 3. Rechnung vom 30.06.2027“ for the invoice choice.
fn invoice_label(index: usize, invoice: &proava_domain::Invoice) -> String {
    let number = if invoice.number.trim().is_empty() {
        format!("Rechnung {}", index + 1)
    } else {
        invoice.number.trim().to_string()
    };
    let date = if invoice.date.trim().is_empty() {
        String::new()
    } else {
        format!(" vom {}", invoice.date.trim())
    };
    format!("{number} – {}{date}", invoice.kind.label())
}

fn shorten(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        text.to_string()
    } else {
        format!("{}…", chars[..max].iter().collect::<String>())
    }
}

/// Titles to print for the selection in the LV: marked titles, or the title
/// that contains a marked position or hint.
pub fn titles_of_selection(project: &Project, selected: &[RowId]) -> Vec<RowId> {
    let lv = project.lv();
    let mut titles: Vec<RowId> = Vec::new();
    for id in selected {
        let Some(index) = lv.index_of(*id) else {
            continue;
        };
        let title = if lv.rows()[index].is_group() {
            Some(*id)
        } else {
            lv.rows()[..index]
                .iter()
                .rev()
                .find(|row| row.is_group())
                .map(|row| row.id())
        };
        if let Some(title) = title {
            if !titles.contains(&title) {
                titles.push(title);
            }
        }
    }
    titles.sort_by_key(|id| lv.index_of(*id));
    titles
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, Lv, LvGroup, OzMask, Position, Project, ProjectName};

    use super::titles_of_selection;

    #[test]
    fn selection_gives_titles_in_document_order() {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        let first = lv.push_group(LvGroup::new(0, "01", "A"));
        let position = lv.push_position(Position::new("01.0010", "x", Decimal::ONE, "St", None));
        let second = lv.push_group(LvGroup::new(0, "02", "B"));
        let project = Project::new(ProjectName::parse("P").unwrap(), "", lv);
        assert_eq!(
            titles_of_selection(&project, &[second, position, first]),
            vec![first, second]
        );
    }
}
