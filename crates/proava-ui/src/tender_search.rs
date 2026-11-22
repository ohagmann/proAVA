//! „In AUSSCHREIBEN.DE suchen“ inside the take-over dialog: searches the
//! catalogue lists AUSSCHREIBEN.DE publishes (`tender_index`), shows title,
//! catalogue, folder and article number, and opens the chosen texts in the
//! browser. There the text is exported as GAEB; the dialog takes the file
//! from the Downloads folder into the LV. With proAVA's own browser window
//! each hit opens there („Öffnen“) and the export comes back directly. The
//! lists hold no long text – a
//! preview and a take-over without the browser would need the partner
//! interface of AUSSCHREIBEN.DE (`docs/FACHLICHES.md`).

use std::path::Path;

use eframe::egui::{self, RichText};
use proava_application::tender_index::{self, SearchResult};
use proava_application::Launcher;

use crate::jobs::{self, Job, Poll};
use crate::theme::Palette;

/// At most this many pages are opened in the browser at once.
const MAX_OPEN: usize = 10;

#[derive(Default)]
pub struct TenderSearch {
    query: String,
    job: Option<Job<Result<SearchResult, String>>>,
    result: Option<SearchResult>,
    chosen: Vec<bool>,
    error: Option<String>,
    note: Option<String>,
}

impl TenderSearch {
    /// Draws the search; returns a page to open in proAVA's browser window
    /// (`own_window`), otherwise pages open in the system browser here.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        launcher: &Launcher,
        own_window: bool,
    ) -> Option<String> {
        self.poll(ui.ctx());
        let mut start = false;
        ui.horizontal(|ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Hersteller und Begriffe, z. B. Belimo Drehantrieb 20 Nm")
                    .desired_width(ui.available_width() - 110.0),
            );
            let enter = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            let button = ui.add_enabled(self.job.is_none(), egui::Button::new("Suchen"));
            start = enter || button.clicked();
        });
        if start && self.job.is_none() {
            self.start(launcher);
        }
        if let Some(job) = &self.job {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(job.label())
                        .size(12.0)
                        .color(palette.text_dim),
                );
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).size(12.0).color(palette.warn));
        }
        let Some(result) = self.result.clone() else {
            ui.label(
                RichText::new(
                    "Gesucht wird in den Katalogverzeichnissen, die AUSSCHREIBEN.DE veröffentlicht (Titel und Artikelnummer, ohne Langtext). Den Text auf seiner Seite als GAEB exportieren – er wird dann hier übernommen.",
                )
                .size(11.5)
                .color(palette.text_dim),
            );
            return None;
        };
        self.hits(ui, palette, &result, own_window)
    }

    fn start(&mut self, launcher: &Launcher) {
        let query = self.query.trim().to_string();
        if query.is_empty() {
            self.error = Some("Bitte Suchbegriffe eingeben.".into());
            return;
        }
        let web = launcher.shared_web();
        self.error = None;
        self.note = None;
        self.job = Some(Job::spawn(
            format!("AUSSCHREIBEN.DE: Verzeichnisse für „{query}“ werden gelesen …"),
            move || tender_index::search(web.as_ref(), &query),
        ));
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.job else {
            return;
        };
        match job.poll() {
            Poll::Running => {}
            Poll::Done(Ok(result)) => {
                self.chosen = vec![false; result.hits.len()];
                if result.hits.is_empty() {
                    self.error = Some(
                        "Keine Treffer – andere Begriffe oder nur den Hersteller eingeben.".into(),
                    );
                }
                self.result = Some(result);
                self.job = None;
                ctx.request_repaint();
            }
            Poll::Done(Err(error)) => {
                self.error = Some(error);
                self.result = None;
                self.job = None;
            }
            Poll::Lost => {
                self.error = Some("Die Suche wurde abgebrochen.".into());
                self.job = None;
            }
        }
    }

    fn hits(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        result: &SearchResult,
        own_window: bool,
    ) -> Option<String> {
        let mut open_here = None;
        let catalogs: Vec<String> = result.catalogs.iter().map(|c| c.name.clone()).collect();
        ui.label(
            RichText::new(format!(
                "{} Treffer in {}{}",
                result.hits.len(),
                catalogs.join(", "),
                if result.truncated {
                    " (gekürzt – Suche verfeinern)"
                } else {
                    ""
                }
            ))
            .size(12.0)
            .color(palette.text_dim),
        );
        // Fixed widths, long titles cut (full text in the tooltip): the
        // dialog must not grow with the titles.
        let width = ui.available_width();
        let article_width = 120.0;
        let folder_width = (width * 0.3).min(240.0);
        let title_width = (width - article_width - folder_width - 60.0).max(120.0);
        let cell = |ui: &mut egui::Ui, width: f32, text: RichText, hover: &str| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 18.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(width);
                    let label = ui.add(egui::Label::new(text).truncate());
                    if !hover.is_empty() {
                        label.on_hover_text(hover);
                    }
                },
            );
        };
        ui.horizontal(|ui| {
            ui.add_space(if own_window { 56.0 } else { 28.0 });
            let head = |text: &str| {
                RichText::new(text)
                    .size(12.0)
                    .strong()
                    .color(palette.text_dim)
            };
            cell(ui, title_width, head("Titel"), "");
            cell(ui, article_width, head("Artikelnummer"), "");
            cell(ui, folder_width, head("Katalog › Ordner"), "");
        });
        egui::ScrollArea::vertical()
            .id_salt("tender_hits")
            .max_height(200.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (index, hit) in result.hits.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if own_window {
                            if ui
                                .small_button("Öffnen")
                                .on_hover_text(
                                    "Im Browserfenster öffnen – dort als GAEB exportieren.",
                                )
                                .clicked()
                            {
                                open_here = Some(hit.url.clone());
                            }
                        } else if let Some(chosen) = self.chosen.get_mut(index) {
                            ui.checkbox(chosen, "");
                        }
                        cell(
                            ui,
                            title_width,
                            RichText::new(&hit.title).size(12.5).color(palette.text),
                            &format!("{}\n{}", hit.title, hit.url),
                        );
                        cell(
                            ui,
                            article_width,
                            RichText::new(&hit.article)
                                .monospace()
                                .size(12.0)
                                .color(palette.text_dim),
                            &hit.article,
                        );
                        let folder = format!("{} › {}", hit.catalog, hit.folder);
                        cell(
                            ui,
                            folder_width,
                            RichText::new(&folder).size(12.0).color(palette.text_dim),
                            &folder,
                        );
                    });
                }
            });
        if own_window {
            if let Some(catalog) = result.catalogs.first() {
                if ui.button("Katalog öffnen").clicked() {
                    open_here = Some(catalog.page_url());
                }
            }
            return open_here;
        }
        let count = self.chosen.iter().filter(|chosen| **chosen).count();
        ui.horizontal_wrapped(|ui| {
            let open = ui
                .add_enabled(count > 0, egui::Button::new(format!("Gewählte im Browser öffnen ({count})")))
                .on_hover_text("Je Text eine Seite – dort als GAEB exportieren; die Datei wird hier übernommen.");
            if open.clicked() {
                let urls: Vec<String> = result
                    .hits
                    .iter()
                    .zip(&self.chosen)
                    .filter(|(_, chosen)| **chosen)
                    .map(|(hit, _)| hit.url.clone())
                    .take(MAX_OPEN)
                    .collect();
                self.open_all(&urls);
            }
            if let Some(catalog) = result.catalogs.first() {
                if ui.button("Katalog im Browser öffnen").clicked() {
                    self.open_all(&[catalog.page_url()]);
                }
            }
        });
        if let Some(note) = &self.note {
            ui.label(RichText::new(note).size(12.0).color(palette.text_dim));
        }
        open_here
    }

    fn open_all(&mut self, urls: &[String]) {
        for url in urls {
            if let Err(error) = jobs::open_with_system(Path::new(url)) {
                self.error = Some(error);
                return;
            }
        }
        self.note = Some(format!(
            "{} geöffnet – dort „Export“ → GAEB wählen (oder auf die Merkliste setzen und gesammelt exportieren).{}",
            crate::format::count(urls.len(), "Seite", "Seiten"),
            if urls.len() >= MAX_OPEN { " Höchstens 10 auf einmal." } else { "" }
        ));
    }
}
