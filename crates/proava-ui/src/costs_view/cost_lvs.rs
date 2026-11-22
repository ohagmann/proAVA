//! The Kosten-LVs in the Reiter Kosten: Kostenschätzungen and
//! Kostenberechnungen as Kurztext-LV (the normal case; Anwender 18.11.). They
//! are edited in the Reiter LV; here they are listed with their sum and can be
//! opened or newly created.

use eframe::egui::{self, RichText};
use proava_domain::{CostStage, Project};

use super::{CostRequest, CostsOutput};
use crate::icons::Icon;
use crate::lv_properties::lv_label_with_stage;
use crate::theme::Palette;
use crate::widgets;

/// One line: „Nach Positionen (Kosten-LV)“, each Kosten-LV as a button
/// (opens it in the Reiter LV) with its net sum, then „Kostenschätzung …“ and
/// „Kostenberechnung …“ for a new one.
pub(super) fn strip(
    ui: &mut egui::Ui,
    palette: &Palette,
    project: &Project,
    output: &mut CostsOutput,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new("Nach Positionen (Kosten-LV)")
                .size(12.0)
                .color(palette.text_dim),
        )
        .on_hover_text(
            "Kostenschätzung und Kostenberechnung als Kurztext-LV: Menge × Kostenkennwert je \
             Bezugseinheit, gegliedert nach Kostengruppen – bearbeitet im Reiter LV",
        );
        let indices = project.cost_lv_indices();
        if indices.is_empty() {
            ui.label(RichText::new("keine").size(12.0).color(palette.text_dim));
        }
        for index in indices {
            let Some(lv) = project.lv_at(index) else {
                continue;
            };
            let text = format!(
                "{} · {} €",
                lv_label_with_stage(lv),
                proava_domain::format_decimal_de(lv.net_total().to_decimal(), 2)
            );
            if widgets::small_button(ui, palette, &text)
                .on_hover_text("Im Reiter LV öffnen")
                .clicked()
            {
                output.requests.push(CostRequest::OpenLv(index));
            }
        }
        ui.add_space(6.0);
        for stage in CostStage::ALL {
            if widgets::icon_label_button(ui, palette, Icon::Plus, &format!("{} …", stage.label()))
                .on_hover_text(format!(
                    "Neue {} als Kurztext-LV (DIN 276, {}. Ebene), Titel nach Kostengruppen",
                    stage.label(),
                    stage.level()
                ))
                .clicked()
            {
                output.requests.push(CostRequest::NewLv(stage));
            }
        }
    });
}

/// The empty Reiter: both ways of a Kostenermittlung side by side.
pub(super) fn choices(
    ui: &mut egui::Ui,
    palette: &Palette,
    output: &mut CostsOutput,
    mut new_points: impl FnMut(CostStage, &mut CostsOutput),
) {
    let width = 250.0;
    let block = |ui: &mut egui::Ui, heading: &str, text: &str| {
        ui.label(
            RichText::new(heading)
                .size(14.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(3.0);
        ui.label(RichText::new(text).size(12.5).color(palette.text_dim));
        ui.add_space(8.0);
    };
    ui.vertical_centered(|ui| {
        ui.set_max_width(2.0 * width + 40.0);
        block(
            ui,
            "Nach Positionen (Kurztext-LV)",
            "Für alle Gewerke: Menge × Kostenkennwert je Bezugseinheit (m Rohrleitung, St. Ventil, \
             kW Heizleistung …), gegliedert nach Kostengruppen. Bearbeitet im Reiter LV wie jedes LV.",
        );
        ui.horizontal(|ui| {
            for stage in CostStage::ALL {
                if widgets::button(
                    ui,
                    palette,
                    &format!("{} als Kurztext-LV …", stage.label()),
                    stage == CostStage::Calculation,
                    width,
                )
                .on_hover_text(format!("DIN 276, {}. Ebene", stage.level()))
                .clicked()
                {
                    output.requests.push(CostRequest::NewLv(stage));
                }
                ui.add_space(12.0);
            }
        });
        ui.add_space(18.0);
        block(
            ui,
            "Nach Datenpunkten (Gebäudeautomation, KG 480)",
            "Kostenschätzung: Datenpunkte × Kostenkennwert = KG 480. Kostenberechnung: Datenpunkte \
             × EP je DP-Art, aufgeteilt auf KG 481–489. Die Datenpunkte stehen je Bauteil in einer \
             Anlagenliste.",
        );
        ui.horizontal(|ui| {
            for stage in CostStage::ALL {
                if widgets::button(
                    ui,
                    palette,
                    &format!("{} nach Datenpunkten", stage.label()),
                    false,
                    width,
                )
                .on_hover_text(stage.template_name())
                .clicked()
                {
                    new_points(stage, output);
                }
                ui.add_space(12.0);
            }
        });
    });
}
