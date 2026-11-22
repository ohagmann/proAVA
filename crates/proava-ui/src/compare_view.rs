//! „Mit anderem Stand vergleichen“: the active LV against another LV of the
//! project, another project or a GAEB file – list of differences with
//! filter, jump into the table, copy as text, PDF and marks in the table.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText};
use proava_domain::lv_diff::{compare, ChangeKind, LvDiff, RowChange};
use proava_domain::{Lv, Project, RowId};

use crate::theme::Palette;

pub struct CompareView {
    other: Option<Other>,
    diff: LvDiff,
    error: Option<String>,
    filter: HashSet<ChangeKind>,
    search: String,
    /// Mark the changed rows in the table.
    pub highlight: bool,
    open: bool,
}

struct Other {
    /// „LV 2 – Elektro“ or the file name (with its LV).
    label: String,
    lv: Lv,
    path: Option<PathBuf>,
}

pub enum CompareRequest {
    PickFile,
    Load(PathBuf),
    /// Another LV of the open project (index).
    OtherLv(usize),
    Jump(RowId),
    Pdf,
}

impl Default for CompareView {
    fn default() -> Self {
        Self::new()
    }
}

impl CompareView {
    pub fn new() -> Self {
        Self {
            other: None,
            diff: LvDiff::default(),
            error: None,
            filter: [
                ChangeKind::Added,
                ChangeKind::Removed,
                ChangeKind::Moved,
                ChangeKind::Changed,
            ]
            .into(),
            search: String::new(),
            highlight: false,
            open: true,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn reopen(&mut self) {
        self.open = true;
    }

    pub fn diff(&self) -> &LvDiff {
        &self.diff
    }

    /// What the other state is called (title block of the PDF).
    pub fn against(&self) -> String {
        self.other
            .as_ref()
            .map(|other| other.label.clone())
            .unwrap_or_default()
    }

    pub fn error(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Sets the other state and compares.
    pub fn set_other(&mut self, label: String, lv: Lv, path: Option<PathBuf>, current: &Lv) {
        self.other = Some(Other { label, lv, path });
        self.error = None;
        self.refresh(current);
    }

    /// Compares again (after changes in the open LV).
    pub fn refresh(&mut self, current: &Lv) {
        if let Some(other) = &self.other {
            self.diff = compare(&other.lv, current);
        }
    }

    /// Rows of the current LV to mark in the table.
    pub fn marked_rows(&self) -> HashSet<RowId> {
        if !self.open || !self.highlight {
            return HashSet::new();
        }
        self.visible().filter_map(|change| change.new_id).collect()
    }

    fn visible(&self) -> impl Iterator<Item = &RowChange> {
        let needle = self.search.trim().to_lowercase();
        self.diff.changes.iter().filter(move |change| {
            self.filter.contains(&change.kind)
                && (needle.is_empty() || change.line().to_lowercase().contains(&needle))
        })
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        project: &Project,
        recent: &[PathBuf],
    ) -> Option<CompareRequest> {
        let mut request = None;
        let mut open = self.open;
        // Esc closes it right after opening, no click first (Anwender 18.11.).
        if open && crate::widgets::esc_closes_window(ctx) {
            open = false;
        }
        egui::Window::new("Mit anderem Stand vergleichen")
            .id(egui::Id::unique("compare_window"))
            .open(&mut open)
            .default_size([680.0, 640.0])
            .default_pos(ctx.content_rect().right_top() + egui::vec2(-720.0, 130.0))
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.set_min_size(egui::vec2(600.0, 480.0));
                request = self.content(ui, palette, project, recent);
            });
        self.open = open;
        request
    }

    fn content(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
        recent: &[PathBuf],
    ) -> Option<CompareRequest> {
        let mut request = None;
        crate::widgets::row(ui, |ui| {
            ui.label(RichText::new("Gegen").size(12.5).color(palette.text_dim));
            let current = self
                .other
                .as_ref()
                .map(|other| other.label.clone())
                .unwrap_or_else(|| "– bitte wählen –".to_string());
            egui::ComboBox::from_id_salt("compare_source")
                .width(380.0)
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for (index, lv) in project.lvs().enumerate() {
                        if index == project.active_lv() {
                            continue;
                        }
                        if ui
                            .selectable_label(false, format!("Dieses Projekt: {}", lv_label(lv)))
                            .clicked()
                        {
                            request = Some(CompareRequest::OtherLv(index));
                        }
                    }
                    for path in recent {
                        if ui
                            .selectable_label(false, file_label(path))
                            .on_hover_text(path.display().to_string())
                            .clicked()
                        {
                            request = Some(CompareRequest::Load(path.clone()));
                        }
                    }
                });
            if ui.button("Datei wählen …").clicked() {
                request = Some(CompareRequest::PickFile);
            }
        });
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).size(12.5).color(palette.error));
        }
        let Some(other) = &self.other else {
            ui.add_space(16.0);
            ui.label(
                RichText::new(
                    "Das offene LV wird mit einem anderen Stand verglichen: einem anderen LV dieses \
                     Projekts, einem anderen Projekt oder einer GAEB-Datei (z. B. der an die Bieter \
                     verschickten X83). Der andere Stand wird nur gelesen.",
                )
                .color(palette.text_dim),
            );
            return request;
        };
        let reload = other.path.clone();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Ergebnis: {}", self.diff.summary()))
                    .size(13.5)
                    .color(palette.text),
            );
            if ui
                .button("Neu vergleichen")
                .on_hover_text("Nach Änderungen im LV")
                .clicked()
            {
                if let Some(path) = &reload {
                    request = Some(CompareRequest::Load(path.clone()));
                }
            }
        });
        crate::widgets::row_wrapped(ui, |ui| {
            for kind in [
                ChangeKind::Added,
                ChangeKind::Removed,
                ChangeKind::Moved,
                ChangeKind::Changed,
            ] {
                let mut shown = self.filter.contains(&kind);
                let label = format!("{} ({})", kind.label(), self.diff.count(kind));
                if ui.checkbox(&mut shown, label).changed() {
                    if shown {
                        self.filter.insert(kind);
                    } else {
                        self.filter.remove(&kind);
                    }
                }
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Filter …")
                    .desired_width(160.0),
            );
        });
        ui.separator();
        let rows: Vec<RowChange> = self.visible().cloned().collect();
        let height = (ui.available_height() - 60.0).max(160.0);
        egui::ScrollArea::vertical()
            .id_salt("compare_list")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(
                        RichText::new("Keine Unterschiede (mit diesem Filter).")
                            .color(palette.text_dim),
                    );
                }
                for change in &rows {
                    let color = match change.kind {
                        ChangeKind::Added => palette.accent,
                        ChangeKind::Removed => palette.error,
                        ChangeKind::Moved | ChangeKind::Changed => palette.warn,
                    };
                    let ordinal = match change.kind {
                        ChangeKind::Moved => {
                            format!("{} → {}", change.old_ordinal, change.new_ordinal)
                        }
                        _ => change.ordinal().to_string(),
                    };
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [76.0, 18.0],
                            egui::Label::new(
                                RichText::new(change.kind.label()).size(12.5).color(color),
                            ),
                        );
                        let text = RichText::new(format!("{ordinal}  {}", change.text))
                            .size(13.0)
                            .color(palette.text);
                        let response = ui.add(
                            egui::Label::new(text)
                                .sense(egui::Sense::click())
                                .truncate(),
                        );
                        if let Some(id) = change.new_id {
                            if response.on_hover_text("In der Tabelle zeigen").clicked() {
                                request = Some(CompareRequest::Jump(id));
                            }
                        }
                    });
                    for detail in change
                        .details
                        .iter()
                        .filter(|d| d.aspect != proava_domain::lv_diff::Aspect::Ordinal)
                    {
                        ui.horizontal(|ui| {
                            ui.add_space(80.0);
                            ui.label(
                                RichText::new(detail.line())
                                    .size(12.0)
                                    .color(palette.text_dim),
                            );
                        });
                    }
                    ui.add_space(3.0);
                }
            });
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.highlight,
                "Geänderte Zeilen in der Tabelle markieren",
            );
            if ui.button("Als Text kopieren").clicked() {
                let text = format!(
                    "Änderungen gegenüber {}: {}\n{}",
                    self.against(),
                    self.diff.summary(),
                    rows.iter()
                        .map(RowChange::line)
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                ui.ctx().copy_text(text);
            }
            if ui
                .button("Drucken …")
                .on_hover_text("Änderungsliste als PDF (Druckdialog)")
                .clicked()
            {
                request = Some(CompareRequest::Pdf);
            }
        });
        request
    }
}

pub fn lv_label(lv: &Lv) -> String {
    if lv.number().trim().is_empty() {
        lv.title().to_string()
    } else {
        format!("LV {} – {}", lv.number().trim(), lv.title())
    }
}

pub fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proava_domain::{Decimal, LvGroup, OzMask, Position};

    fn lv(quantity: i64) -> Lv {
        let mut lv = Lv::new("LV", OzMask::parse("11.PPPP").unwrap());
        lv.push_group(LvGroup::new(0, "", "Kabel"));
        lv.push_position(Position::new(
            "",
            "Abzweigkasten",
            Decimal::from(quantity),
            "St",
            None,
        ));
        lv.push_position(Position::new("", "Ventil", Decimal::ONE, "St", None));
        lv.renumber();
        lv
    }

    #[test]
    fn filter_search_and_marks() {
        let old = lv(652);
        let mut current = lv(150);
        current.remove_row(2);
        let mut view = CompareView::new();
        view.set_other("F.X83".into(), old, None, &current);
        assert_eq!(view.diff().summary(), "1 entfallen, 1 geändert");
        assert!(view.marked_rows().is_empty(), "marks only when asked");
        view.highlight = true;
        assert_eq!(view.marked_rows(), [current.rows()[1].id()].into());
        view.filter.remove(&ChangeKind::Changed);
        assert!(
            view.marked_rows().is_empty(),
            "removed rows are not in the table"
        );
        view.filter.insert(ChangeKind::Changed);
        view.search = "ventil".into();
        assert_eq!(view.visible().count(), 1);
        assert_eq!(view.against(), "F.X83");
    }
}
