//! Dialogs of the Vergabe-Abschluss: award a bid, export the Auftrags-LV,
//! carry prices into the planner's prices.

use eframe::egui;
use proava_application::ContractExport;
use proava_domain::{BidId, PriceSource, Project};

use crate::theme::Palette;
use crate::view::{ViewRequest, ViewResponse};
use crate::widgets;

/// Which dialog is open.
#[derive(Debug, Clone, PartialEq)]
pub enum AwardDialog {
    Award {
        bid: BidId,
        date: String,
        error: Option<String>,
    },
    Contract {
        bid: BidId,
        kind: ContractExport,
    },
    Prices {
        /// None = from the selected bid (`bid`), else a statistic.
        source: PriceSource,
        bid: BidId,
        overwrite: bool,
        error: Option<String>,
    },
}

impl AwardDialog {
    /// Award dialog for `bid`, the date preset to `today` (TT.MM.JJJJ).
    pub fn award(bid: BidId, today: &str) -> Self {
        AwardDialog::Award {
            bid,
            date: today.to_string(),
            error: None,
        }
    }

    pub fn contract(project: &Project, selected: BidId) -> Self {
        AwardDialog::Contract {
            bid: project
                .awarded_bid()
                .map(|bid| bid.id())
                .unwrap_or(selected),
            kind: ContractExport::Xml,
        }
    }

    pub fn prices(bid: BidId) -> Self {
        AwardDialog::Prices {
            source: PriceSource::Bid(bid),
            bid,
            overwrite: false,
            error: None,
        }
    }
}

fn bid_name(project: &Project, id: BidId) -> String {
    project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .map(|bid| bid.name().to_string())
        .unwrap_or_default()
}

/// „Im Angebot fehlen 2 Preise (01.01.0010, 01.01.0020) …“ – the award of an
/// incomplete bid should be a conscious decision.
fn missing_prices_hint(project: &Project, id: BidId) -> Option<String> {
    let bid = project.bids().iter().find(|bid| bid.id() == id)?;
    let missing = bid.missing_positions(project.lv());
    if missing.is_empty() {
        return None;
    }
    let mut ordinals: Vec<&str> = missing
        .iter()
        .take(5)
        .map(|position| position.ordinal())
        .collect();
    if missing.len() > 5 {
        ordinals.push("…");
    }
    Some(format!(
        "Im Angebot {} {} ({}). Diese Leistungen sind nicht beauftragt – bitte vorher klären.",
        if missing.len() == 1 {
            "fehlt"
        } else {
            "fehlen"
        },
        crate::format::count(missing.len(), "Preis", "Preise"),
        ordinals.join(", ")
    ))
}

/// Draws the open dialog; returns true when it is finished (closed).
pub fn show(
    dialog: &mut AwardDialog,
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &mut Project,
    response: &mut ViewResponse,
) -> bool {
    match dialog {
        AwardDialog::Award { bid, date, error } => {
            let id = *bid;
            let name = bid_name(project, id);
            let previous = project
                .awarded_bid()
                .filter(|other| other.id() != id)
                .map(|other| other.name().to_string());
            let missing = missing_prices_hint(project, id);
            let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 440.0, |ui| {
                let mut hint = format!(
                    "„{name}“ bekommt den Auftrag. Die Auftragssumme ist die Angebotssumme netto nach Nachlass."
                );
                if let Some(previous) = &previous {
                    hint.push_str(&format!(
                        " Der Auftrag an „{previous}“ wird damit aufgehoben."
                    ));
                }
                widgets::title(ui, palette, "Auftrag erteilen", &hint);
                if let Some(missing) = &missing {
                    ui.label(egui::RichText::new(missing).size(12.5).color(palette.warn));
                    ui.add_space(6.0);
                }
                ui.allocate_ui(egui::vec2(220.0, 70.0), |ui| {
                    crate::form_kit::date_field(ui, palette, "Datum", date, None);
                });
                ui.add_space(10.0);
                widgets::error_line(ui, palette, error.as_deref());
                widgets::dialog_buttons(ui, palette, "Auftrag erteilen")
            });
            if cancelled || close {
                return true;
            }
            if confirmed {
                if date.trim().is_empty() {
                    *error = Some("Bitte das Datum der Auftragserteilung eintragen.".to_string());
                    return false;
                }
                match project.award_bid(id, date) {
                    Ok(total) => {
                        response.dirty = true;
                        response.toast = Some(format!(
                            "Auftrag an {name} erteilt – Auftragssumme netto {total}"
                        ));
                        return true;
                    }
                    Err(message) => *error = Some(message),
                }
            }
            false
        }
        AwardDialog::Contract { bid, kind } => {
            let name = bid_name(project, *bid);
            let awarded = project
                .bids()
                .iter()
                .find(|other| other.id() == *bid)
                .is_some_and(|other| other.is_awarded());
            let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 440.0, |ui| {
                let mut hint = format!(
                    "LV mit den Preisen, dem Nachlass und den Angaben von „{name}“, mit Auftraggeber und Auftragnehmer."
                );
                if !awarded {
                    hint.push_str(" Achtung: Dieses Angebot ist noch nicht beauftragt.");
                }
                widgets::title(ui, palette, "Auftrags-LV exportieren", &hint);
                for choice in [
                    ContractExport::Xml,
                    ContractExport::Datan,
                    ContractExport::Pdf,
                ] {
                    ui.radio_value(kind, choice, choice.label());
                }
                ui.add_space(10.0);
                widgets::dialog_buttons(ui, palette, "Weiter …")
            });
            if cancelled || close {
                return true;
            }
            if confirmed {
                response
                    .requests
                    .push(ViewRequest::ExportContract(*bid, *kind));
                return true;
            }
            false
        }
        AwardDialog::Prices {
            source,
            bid,
            overwrite,
            error,
        } => {
            let name = bid_name(project, *bid);
            let ((cancelled, confirmed), close) = widgets::modal(ui, palette, 460.0, |ui| {
                widgets::title(
                    ui,
                    palette,
                    "Preise als Planerpreise übernehmen",
                    "Schreibt den Kostenvoranschlag fort. Rückgängig mit Strg+Z im LV.",
                );
                ui.radio_value(
                    source,
                    PriceSource::Bid(*bid),
                    format!("aus dem Angebot „{name}“"),
                );
                ui.radio_value(
                    source,
                    PriceSource::Mean,
                    "Mittelwert der gewerteten Angebote",
                );
                ui.radio_value(
                    source,
                    PriceSource::Median,
                    "Median der gewerteten Angebote",
                );
                ui.radio_value(
                    source,
                    PriceSource::Minimum,
                    "Minimum der gewerteten Angebote",
                );
                ui.add_space(8.0);
                ui.checkbox(overwrite, "Vorhandene Planerpreise überschreiben");
                ui.add_space(10.0);
                widgets::error_line(ui, palette, error.as_deref());
                widgets::dialog_buttons(ui, palette, "Übernehmen")
            });
            if cancelled || close {
                return true;
            }
            if confirmed {
                response.requests.push(ViewRequest::AdoptPrices {
                    source: *source,
                    overwrite: *overwrite,
                });
                return true;
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::missing_prices_hint;
    use proava_domain::BidPrice;

    #[test]
    fn awarding_an_incomplete_bid_names_the_missing_prices() {
        let mut project = crate::demo::demo_project();
        let bid = project.bids()[0].id();
        let complete = project.bids()[0].missing_positions(project.lv()).is_empty();
        assert_eq!(missing_prices_hint(&project, bid).is_none(), complete);
        let first = project.lv().positions().next().unwrap();
        let (position, ordinal) = (first.id(), first.ordinal().to_string());
        project.bid_mut(bid).unwrap().set_price(
            position,
            BidPrice {
                unit_price: None,
                stated_total: None,
                not_offered: false,
            },
        );
        let hint = missing_prices_hint(&project, bid).expect("Hinweis");
        assert!(hint.contains(&ordinal), "{hint}");
        assert!(hint.starts_with("Im Angebot fehl"), "{hint}");
    }
}
