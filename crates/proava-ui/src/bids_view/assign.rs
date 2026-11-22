//! „Angebotspositionen zuordnen“ (Anwender 20.11.): Positionen der
//! Angebotsdatei ohne passende OZ – oder deren OZ dem Inhalt widerspricht –
//! einer LV-Position zuweisen. Vorschläge kommen aus dem Strukturvergleich
//! beim Einlesen (Reihenfolge, Menge, Text); „sicher“ ist vorgewählt.

use eframe::egui::{self, RichText};
use proava_domain::{AssignedBy, BidId, BidItem, Project, RowId};

use crate::theme::Palette;
use crate::widgets;

/// One line of the dialog: a file item and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct AssignRow {
    pub ordinal: String,
    /// What the file says: text, quantity, prices.
    pub item: String,
    /// The proposal of the comparison, if any.
    pub proposed: Option<RowId>,
    pub target: Option<RowId>,
    pub similarity: u8,
    pub sure: bool,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AssignDialog {
    pub bid: BidId,
    pub rows: Vec<AssignRow>,
}

/// What the dialog asks for.
pub(super) enum AssignOutcome {
    Open,
    Cancel,
    /// (OZ in the file, LV row, how, similarity).
    Apply(Vec<(String, RowId, AssignedBy, u8)>),
}

fn number(value: proava_domain::Decimal) -> String {
    proava_domain::format_decimal_de(value.normalize(), value.normalize().scale().min(3))
}

/// „Bohrlöcher … · Menge 120 · EP 98,44 · GP 11.812,80“.
fn describe(item: &BidItem) -> String {
    let mut parts = Vec::new();
    if !item.short_text.is_empty() {
        parts.push(item.short_text.clone());
    }
    if let Some(quantity) = item.priced_quantity() {
        let unit = if item.unit.is_empty() {
            String::new()
        } else {
            format!(" {}", item.unit)
        };
        parts.push(format!("Menge {}{unit}", number(quantity)));
    }
    if let Some(price) = item.unit_price {
        parts.push(format!("EP {}", number(price)));
    }
    if let Some(total) = item.total {
        parts.push(format!("GP {}", proava_domain::Money::from_decimal(total)));
    }
    if parts.is_empty() {
        "ohne Angaben".to_string()
    } else {
        parts.join(" · ")
    }
}

impl AssignDialog {
    /// The open proposals and the file items without one.
    pub fn of(project: &Project, bid: BidId) -> Option<Self> {
        let structure = project
            .bids()
            .iter()
            .find(|other| other.id() == bid)?
            .structure();
        let mut rows: Vec<AssignRow> = structure
            .suggestions
            .iter()
            .filter_map(|suggestion| {
                let item = structure.item(&suggestion.ordinal)?;
                Some(AssignRow {
                    ordinal: suggestion.ordinal.clone(),
                    item: describe(item),
                    proposed: Some(suggestion.row),
                    target: Some(suggestion.row),
                    similarity: suggestion.similarity,
                    sure: suggestion.sure,
                    checked: suggestion.sure,
                })
            })
            .collect();
        for item in &structure.foreign {
            if !rows.iter().any(|row| row.ordinal == item.ordinal) {
                rows.push(AssignRow {
                    ordinal: item.ordinal.clone(),
                    item: describe(item),
                    proposed: None,
                    target: None,
                    similarity: 0,
                    sure: false,
                    checked: false,
                });
            }
        }
        (!rows.is_empty()).then_some(Self { bid, rows })
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        palette: &Palette,
        project: &Project,
    ) -> AssignOutcome {
        let Some(bid) = project.bids().iter().find(|other| other.id() == self.bid) else {
            return AssignOutcome::Cancel;
        };
        let lv = project.lv();
        let structure = bid.structure();
        // Candidates: LV positions without a price of this bid, and those
        // whose price came from a contradicted OZ.
        let conflict_rows: Vec<RowId> = structure.conflicts.iter().map(|(row, _)| *row).collect();
        let candidates: Vec<(RowId, String)> = lv
            .positions()
            .filter(|position| {
                bid.price(position.id()).is_none() || conflict_rows.contains(&position.id())
            })
            .map(|position| {
                let text: String = position
                    .short_text()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                (
                    position.id(),
                    format!(
                        "{} {}",
                        position.ordinal(),
                        text.chars().take(60).collect::<String>()
                    ),
                )
            })
            .collect();
        let label_of = |row: Option<RowId>| {
            row.and_then(|row| {
                candidates
                    .iter()
                    .find(|(id, _)| *id == row)
                    .map(|(_, label)| label.clone())
            })
            .unwrap_or_else(|| "– nicht zuordnen –".to_string())
        };
        let mut sure_all = false;
        let mut apply = false;
        let mut cancel = false;
        let ((), close) = widgets::modal_with_id(
            ui,
            palette,
            egui::Id::unique(("proava_assign_bid_items", self.bid.0)),
            900.0,
            |ui| {
                widgets::title(
                    ui,
                    palette,
                    "Angebotspositionen zuordnen",
                    &format!(
                        "{}: Positionen der Datei ohne passende OZ. „sicher“ = gleich viele Positionen an der Stelle, Mengen bzw. Texte bestätigen es. Gewertet wird mit Menge und Text des LV.",
                        bid.name()
                    ),
                );
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        egui::Grid::new(("assign_grid", self.bid.0))
                            .striped(true)
                            .num_columns(5)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                for head in [
                                    "",
                                    "Angebot",
                                    "Angaben der Datei",
                                    "LV-Position",
                                    "Ähnlichkeit",
                                ] {
                                    ui.label(
                                        RichText::new(head).size(11.5).strong().color(palette.text),
                                    );
                                }
                                ui.end_row();
                                for (index, row) in self.rows.iter_mut().enumerate() {
                                    ui.add_enabled(
                                        row.target.is_some(),
                                        egui::Checkbox::without_text(&mut row.checked),
                                    )
                                    .on_hover_text("übernehmen");
                                    ui.label(
                                        RichText::new(&row.ordinal)
                                            .monospace()
                                            .size(11.5)
                                            .color(palette.text),
                                    );
                                    ui.label(
                                        RichText::new(&row.item).size(11.5).color(palette.text),
                                    );
                                    let before = row.target;
                                    egui::ComboBox::from_id_salt((
                                        "assign_target",
                                        self.bid.0,
                                        index,
                                    ))
                                    .width(320.0)
                                    .selected_text(label_of(row.target))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut row.target,
                                            None,
                                            "– nicht zuordnen –",
                                        );
                                        for (id, label) in &candidates {
                                            ui.selectable_value(&mut row.target, Some(*id), label);
                                        }
                                    });
                                    if row.target != before {
                                        // Another choice: the user's, to check.
                                        row.sure = row.target.is_some()
                                            && row.target == row.proposed
                                            && row.sure;
                                        row.checked = row.target.is_some();
                                        row.similarity = row
                                            .target
                                            .and_then(|target| lv.position_by_id(target))
                                            .zip(structure.item(&row.ordinal))
                                            .map(|(position, item)| {
                                                proava_domain::check_pair(item, position).similarity
                                            })
                                            .unwrap_or(0);
                                    }
                                    let verdict = match (row.target, row.sure) {
                                        (None, _) => String::new(),
                                        (Some(_), true) => format!("{} % · sicher", row.similarity),
                                        (Some(_), false) => {
                                            format!("{} % · prüfen", row.similarity)
                                        }
                                    };
                                    ui.label(
                                        RichText::new(verdict).size(11.5).color(if row.sure {
                                            palette.text
                                        } else {
                                            palette.warn
                                        }),
                                    );
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(12.0);
                widgets::button_row(ui, |ui| {
                    if widgets::button(ui, palette, "Übernehmen", true, 130.0).clicked() {
                        apply = true;
                    }
                    ui.add_space(8.0);
                    if widgets::button(ui, palette, "Sichere übernehmen", false, 150.0).clicked() {
                        sure_all = true;
                    }
                    ui.add_space(8.0);
                    if widgets::button(ui, palette, crate::strings::CANCEL, false, 110.0).clicked()
                    {
                        cancel = true;
                    }
                });
            },
        );
        if cancel || close {
            return AssignOutcome::Cancel;
        }
        if sure_all || apply {
            let chosen = self
                .rows
                .iter()
                .filter(|row| if sure_all { row.sure } else { row.checked })
                .filter_map(|row| {
                    let target = row.target?;
                    let by = if row.sure && row.target == row.proposed {
                        AssignedBy::Structure
                    } else {
                        AssignedBy::Hand
                    };
                    Some((row.ordinal.clone(), target, by, row.similarity))
                })
                .collect();
            return AssignOutcome::Apply(chosen);
        }
        AssignOutcome::Open
    }
}
