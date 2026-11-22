//! The LVs of the project: the commands for LVs (menu LV, context menu of an
//! LV head row in the table), the lines with number, title, sum and bids, and
//! the LV choice of the views that work on one LV.

use eframe::egui::{self, RichText};
use proava_domain::Project;

use crate::lv_properties::lv_label;
use crate::strings;
use crate::theme::Palette;

/// What the user wants to do with the LVs of the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LvListAction {
    Switch(usize),
    New,
    Properties(usize),
    Duplicate(usize),
    Move {
        index: usize,
        up: bool,
    },
    /// Asks first (positions and bids are shown).
    Delete(usize),
    /// The whole LV into the clipboard (also for other proAVA windows).
    Copy(usize),
    /// An LV from the clipboard as a new LV (asks about prices).
    Paste,
}

/// One line of the LV list.
#[derive(Debug, Clone, PartialEq)]
pub struct LvLine {
    pub label: String,
    pub net: String,
    pub positions: usize,
    pub bids: usize,
    pub active: bool,
    /// The number is used by another LV too.
    pub duplicate_number: bool,
}

pub fn lines(project: &Project) -> Vec<LvLine> {
    let doubles = project.duplicate_lv_numbers();
    project
        .lvs()
        .enumerate()
        .map(|(index, lv)| LvLine {
            label: lv_label(lv),
            net: if lv.currency() == "EUR" {
                lv.net_total().to_string()
            } else {
                format!("{} {}", lv.net_total().plain(), lv.currency())
            },
            positions: lv.position_count(),
            bids: project.bids_of(index).len(),
            active: index == project.active_lv(),
            duplicate_number: doubles.contains(&lv.number().trim().to_string()),
        })
        .collect()
}

/// Text for the „LV löschen“ question.
pub fn delete_question(project: &Project, index: usize) -> Option<String> {
    let lv = project.lv_at(index)?;
    let bids = project.bids_of(index).len();
    let mut text = format!(
        "„{}“ mit {} Position{} wird aus dem Projekt entfernt",
        lv_label(lv),
        lv.position_count(),
        if lv.position_count() == 1 { "" } else { "en" }
    );
    if bids > 0 {
        text.push_str(&format!(
            " – samt {bids} eingelesene{} Angebot{} und Preisspiegel",
            if bids == 1 { "m" } else { "n" },
            if bids == 1 { "" } else { "en" }
        ));
    }
    text.push_str(". Rückgängig (Strg+Z) holt es zurück.");
    Some(text)
}

/// Whether the views of one LV (Angebote, Preisspiegel …) show the LV
/// choice: with more than one LV of the tender, or when the LV of the
/// Schreibmarke is a Kostenermittlung (then the choice leads to the tender).
pub fn has_view_choice(project: &Project) -> bool {
    project.tender_lv_indices().len() > 1
        || (project.lv_count() > 1 && !project.lv_purpose(project.active_lv()).is_tender())
}

/// The small LV choice above the other views (Angebote, Preisspiegel,
/// Abrechnung, Nachträge, Bieter) when the project has several LVs: those
/// views work on one LV; the LV table itself shows all of them one below the
/// other. Its choice is the LV of the Schreibmarke. Returns the LV picked.
pub fn view_choice(ui: &mut egui::Ui, palette: &Palette, project: &Project) -> Option<usize> {
    if !has_view_choice(project) {
        return None;
    }
    let lines = lines(project);
    let active = project.active_lv();
    // Only LVs of the tender: a Kostenermittlung has no Bieter, Angebote,
    // Preisspiegel, Auftrag or Abrechnung.
    let tender = project.tender_lv_indices();
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(strings::LV_CHOICE_LABEL)
                .size(12.5)
                .color(palette.text_dim),
        );
        let current = lines
            .get(active)
            .map(|line| crate::format::truncate_middle(&line.label, 60))
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("lv_view_choice")
            .selected_text(RichText::new(current).size(13.0).color(palette.text))
            .width(360.0)
            .show_ui(ui, |ui| {
                for (index, line) in lines
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| tender.contains(index))
                {
                    let text = format!(
                        "{}   ({} · {})",
                        crate::format::truncate_middle(&line.label, 60),
                        crate::format::count(line.positions, "Position", "Positionen"),
                        crate::format::count(line.bids, "Angebot", "Angebote"),
                    );
                    if ui.selectable_label(index == active, text).clicked() && index != active {
                        picked = Some(index);
                    }
                }
            })
            .response
            .on_hover_text(strings::LV_CHOICE_HINT);
    });
    picked
}

#[cfg(test)]
mod tests {
    use proava_domain::{Address, Bid, Lv, OzMask, Project, ProjectName};

    use super::{delete_question, lines};

    #[test]
    fn lines_and_delete_question() {
        let mask = || OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut first = Lv::new("Gebäudeautomation", mask());
        first.set_number("1");
        let mut project = Project::new(ProjectName::parse("Schule").unwrap(), "2026-11-17", first);
        project.add_bid(Bid::new(Address::named("A"), "a.x84"));
        let mut second = Lv::new("Elektro", mask());
        second.set_number("1");
        project.add_lv(second);
        let lines = lines(&project);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].label, "LV 1 – Gebäudeautomation");
        assert!(lines[0].active && !lines[1].active);
        assert_eq!(lines[0].bids, 1);
        assert!(lines[1].duplicate_number);
        let question = delete_question(&project, 0).unwrap();
        assert!(question.contains("0 Positionen"));
        assert!(question.contains("1 eingelesenem Angebot"));
        assert!(delete_question(&project, 5).is_none());
    }
}
