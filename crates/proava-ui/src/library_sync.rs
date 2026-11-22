//! „Mit Bibliothek abgleichen“: positions of the LV that came from a library
//! against their entries there – what changed in the library since (Kurz-,
//! Langtext, Einheit, Art, Textergänzungen, wahlweise EP; uses the LV
//! comparison), and taking the library state over for the chosen ones.

use std::collections::HashSet;

use eframe::egui::{self, RichText};
use proava_domain::library::{self, Links};
use proava_domain::lv_diff::{self, ChangeKind, RowChange};
use proava_domain::{Project, RowId};

use crate::theme::Palette;

pub struct LibrarySync {
    /// (name, library) of the libraries at hand.
    libraries: Vec<(String, Project)>,
    links: Links,
    changes: Vec<RowChange>,
    chosen: HashSet<RowId>,
    /// Compare and take the unit prices too.
    pub prices: bool,
    open: bool,
    /// Libraries that could not be read.
    problems: Vec<String>,
}

pub enum SyncRequest {
    /// Take the library state over for these positions.
    Take {
        chosen: Vec<RowId>,
        prices: bool,
    },
    Jump(RowId),
}

impl LibrarySync {
    pub fn new(
        libraries: Vec<(String, Project)>,
        problems: Vec<String>,
        project: &Project,
    ) -> Self {
        let mut sync = Self {
            libraries,
            links: Links::default(),
            changes: Vec::new(),
            chosen: HashSet::new(),
            prices: false,
            open: true,
            problems,
        };
        sync.refresh(project);
        sync.chosen = sync
            .changes
            .iter()
            .filter_map(|change| change.new_id)
            .collect();
        sync
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn libraries(&self) -> Vec<(&str, &Project)> {
        self.libraries
            .iter()
            .map(|(name, project)| (name.as_str(), project))
            .collect()
    }

    pub fn links(&self) -> &Links {
        &self.links
    }

    /// Positions that differ from their library entry.
    pub fn changes(&self) -> &[RowChange] {
        &self.changes
    }

    /// Compares again (after taking over, after undo, prices switched).
    pub fn refresh(&mut self, project: &Project) {
        let lvs: Vec<(&str, &proava_domain::Lv)> = self
            .libraries
            .iter()
            .map(|(name, library)| (name.as_str(), library.lv()))
            .collect();
        self.links = library::links(project.lv(), &lvs);
        let state = library::library_state(project.lv(), &lvs, &self.links, self.prices);
        self.changes = lv_diff::compare(project.lv(), &state)
            .changes
            .into_iter()
            .filter(|change| change.kind == ChangeKind::Changed)
            .collect();
        let present: HashSet<RowId> = self
            .changes
            .iter()
            .filter_map(|change| change.new_id)
            .collect();
        self.chosen.retain(|id| present.contains(id));
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &Project,
    ) -> Option<SyncRequest> {
        let mut request = None;
        let mut open = self.open;
        // Esc closes it right after opening, no click first (Anwender 18.11.).
        if open && crate::widgets::esc_closes_window(ctx) {
            open = false;
        }
        egui::Window::new("Mit Bibliothek abgleichen")
            .id(egui::Id::unique("library_sync"))
            .open(&mut open)
            .default_size([640.0, 560.0])
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                request = self.content(ui, palette, project);
            });
        self.open = open;
        request
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
    ) -> Option<SyncRequest> {
        let mut request = None;
        let linked = self.links.found.len();
        let summary = format!(
            "{} aus einer Bibliothek · {} vom Bibliotheksstand verschieden{}",
            positions(linked),
            positions(self.changes.len()),
            if self.links.missing.is_empty() {
                String::new()
            } else {
                format!(" · {} ohne Eintrag", self.links.missing.len())
            }
        );
        ui.label(RichText::new(summary).size(12.5).color(palette.text_dim));
        for problem in &self.problems {
            ui.label(RichText::new(problem).size(12.0).color(palette.warn));
        }
        if !self.links.missing.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Eintrag nicht mehr in der Bibliothek (oder Bibliothek nicht eingerichtet): {}",
                    self.links.missing.join(", ")
                ))
                .size(12.0)
                .color(palette.text_dim),
            );
        }
        if ui
            .checkbox(
                &mut self.prices,
                "Auch Einheitspreise vergleichen und übernehmen (Erfahrungspreise)",
            )
            .changed()
        {
            self.refresh(project);
        }
        ui.separator();
        if linked == 0 {
            ui.label(
                RichText::new(
                    "Keine Position dieses LV stammt aus einer Bibliothek. Positionen, die über das \
                     Seitenfenster „Bibliothek“ (F8) eingefügt werden, merken sich ihren Eintrag.",
                )
                .color(palette.text_dim),
            );
            return None;
        }
        if self.changes.is_empty() {
            ui.label(RichText::new("Alles auf dem Stand der Bibliothek.").color(palette.text));
            return None;
        }
        ui.horizontal(|ui| {
            if ui.small_button("Alle wählen").clicked() {
                self.chosen = self
                    .changes
                    .iter()
                    .filter_map(|change| change.new_id)
                    .collect();
            }
            if ui.small_button("Keine").clicked() {
                self.chosen.clear();
            }
        });
        let height = (ui.available_height() - 50.0).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("library_sync_list")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for change in &self.changes {
                    let Some(id) = change.new_id else { continue };
                    let locked = project
                        .lv()
                        .position_by_id(id)
                        .is_some_and(|position| position.is_locked());
                    ui.horizontal(|ui| {
                        let mut ticked = self.chosen.contains(&id);
                        if ui
                            .add_enabled(!locked, egui::Checkbox::new(&mut ticked, ""))
                            .changed()
                        {
                            if ticked {
                                self.chosen.insert(id);
                            } else {
                                self.chosen.remove(&id);
                            }
                        }
                        let label = format!(
                            "{}  {}{}",
                            change.ordinal(),
                            change.text,
                            if locked { " (fertig – gesperrt)" } else { "" }
                        );
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(label).size(13.0).strong().color(palette.text),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .on_hover_text("Klick: im LV zeigen")
                            .clicked()
                        {
                            request = Some(SyncRequest::Jump(id));
                        }
                    });
                    for detail in &change.details {
                        let line = detail.line();
                        ui.indent(("sync_detail", id.0, detail.aspect.label()), |ui| {
                            ui.label(RichText::new(line).size(12.0).color(palette.text_dim));
                        });
                    }
                    ui.add_space(4.0);
                }
            });
        ui.separator();
        let count = self.chosen.len();
        if ui
            .add_enabled(
                count > 0,
                egui::Button::new(
                    RichText::new(format!("Bibliotheksstand übernehmen ({count})")).color(palette.on_accent),
                )
                .fill(palette.accent),
            )
            .on_hover_text("Texte, Einheit, Art und Textergänzungen aus der Bibliothek; Menge, OZ und Status bleiben. Ein Rückgängig-Schritt.")
            .clicked()
        {
            request = Some(SyncRequest::Take {
                chosen: self.chosen.iter().copied().collect(),
                prices: self.prices,
            });
        }
        request
    }
}

/// „1 Position“, „3 Positionen“
fn positions(count: usize) -> String {
    if count == 1 {
        "1 Position".to_string()
    } else {
        format!("{count} Positionen")
    }
}
