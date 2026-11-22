//! „Eigenschaften: LV“ – number, title, OZ mask, step, VAT, currency of one LV;
//! for a new LV and for an existing one. Pure checks, so they are tested
//! without egui.

use eframe::egui::{self, RichText};
use proava_domain::{
    cost_lv_mask, cost_lv_template, format_decimal_de, kg400_groups, parse_decimal_de, CostStage,
    Decimal, Lv, LvPurpose, OzMask, Project,
};

use crate::form_kit;
use crate::theme::Palette;
use crate::{strings, widgets};

/// New LV or the LV at an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LvTarget {
    New,
    Existing(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LvPropertiesForm {
    pub target: LvTarget,
    pub number: String,
    pub title: String,
    pub mask: String,
    pub step: u32,
    pub vat: String,
    pub currency: String,
    /// Pauschalsumme of the LV (GAEB TotalLSUM), German number; empty = none.
    pub lump_sum: String,
    /// Numbers of the other LVs (no doubles allowed).
    other_numbers: Vec<String>,
    /// The existing LV has rows: a changed mask renumbers them.
    has_rows: bool,
    original_mask: String,
    original_step: u32,
    pub error: Option<String>,
    focus: bool,
    /// Stufen-Bezeichnungen from the Gliederungs-Assistent (new LV only).
    pub level_labels: Option<Vec<String>>,
    /// The user pressed „Gliederung …“: the workspace opens the assistant.
    pub wants_assistant: bool,
    /// Ausschreibung or Kostenermittlung (Kostenschätzung, Kostenberechnung).
    pub purpose: LvPurpose,
    /// New Kosten-LV: the Kostengruppen of the 2nd level (410 … 490) that get
    /// a title (Kostenberechnung: with subtitles of the 3rd level).
    pub cost_groups: Vec<(String, bool)>,
    /// What of the tender the existing LV has („2 Angebote, Auftrag“): then it
    /// stays a tender LV.
    tender_data: String,
    /// Mask of the template LV, back when a new LV turns into a tender LV again.
    tender_mask: String,
}

/// Choices of the field „Art“.
pub const PURPOSES: [(LvPurpose, &str); 3] = [
    (LvPurpose::Tender, "Ausschreibung"),
    (LvPurpose::Cost(CostStage::Estimate), "Kostenschätzung"),
    (LvPurpose::Cost(CostStage::Calculation), "Kostenberechnung"),
];

/// All Kostengruppen of the 2nd level of KG 400, chosen.
fn all_cost_groups() -> Vec<(String, bool)> {
    kg400_groups()
        .into_iter()
        .map(|(code, _)| (code.to_string(), true))
        .collect()
}

/// Field errors of the form.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LvErrors {
    pub number: Option<String>,
    pub title: Option<String>,
    pub mask: Option<String>,
    pub vat: Option<String>,
    pub currency: Option<String>,
    pub lump_sum: Option<String>,
}

impl LvErrors {
    pub fn is_empty(&self) -> bool {
        self.number.is_none()
            && self.title.is_none()
            && self.mask.is_none()
            && self.vat.is_none()
            && self.currency.is_none()
            && self.lump_sum.is_none()
    }
}

impl LvPropertiesForm {
    /// For a new LV: mask, step, VAT and currency of the active LV as template,
    /// the next free number.
    pub fn new_lv(project: &Project) -> Self {
        let template = project.lv();
        Self {
            target: LvTarget::New,
            number: next_lv_number(project),
            title: String::new(),
            mask: template.oz_mask().pattern().to_string(),
            step: template.position_step(),
            vat: vat_text(template.vat_percent()),
            currency: template.currency().to_string(),
            lump_sum: String::new(),
            other_numbers: project
                .lvs()
                .map(|lv| lv.number().trim().to_string())
                .collect(),
            has_rows: false,
            original_mask: template.oz_mask().pattern().to_string(),
            original_step: template.position_step(),
            error: None,
            focus: true,
            level_labels: None,
            wants_assistant: false,
            purpose: LvPurpose::Tender,
            cost_groups: all_cost_groups(),
            tender_data: String::new(),
            tender_mask: template.oz_mask().pattern().to_string(),
        }
    }

    /// For a new Kosten-LV of this Stufe (Reiter Kosten, „Neue Kostenermittlung“):
    /// mask and titles after DIN 276, KG 400.
    pub fn new_cost_lv(project: &Project, stage: CostStage) -> Self {
        let mut form = Self::new_lv(project);
        form.set_purpose(LvPurpose::Cost(stage));
        form.title = format!("{} KG 400", stage.label());
        form
    }

    /// Sets the Art; a new LV gets the mask that fits it.
    pub fn set_purpose(&mut self, purpose: LvPurpose) {
        self.purpose = purpose;
        if self.target == LvTarget::New {
            self.mask = match purpose.cost_stage() {
                Some(stage) => cost_lv_mask(stage).pattern().to_string(),
                None => self.tender_mask.clone(),
            };
            self.level_labels = None;
        }
    }

    pub fn existing(project: &Project, index: usize) -> Option<Self> {
        let lv = project.lv_at(index)?;
        Some(Self {
            target: LvTarget::Existing(index),
            number: lv.number().to_string(),
            title: lv.title().to_string(),
            mask: lv.oz_mask().pattern().to_string(),
            step: lv.position_step(),
            vat: vat_text(lv.vat_percent()),
            currency: lv.currency().to_string(),
            lump_sum: lv
                .lump_sum_total()
                .map(|total| format_decimal_de(total.to_decimal(), 2))
                .unwrap_or_default(),
            other_numbers: project
                .lvs()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, lv)| lv.number().trim().to_string())
                .collect(),
            has_rows: !lv.rows().is_empty(),
            original_mask: lv.oz_mask().pattern().to_string(),
            original_step: lv.position_step(),
            error: None,
            focus: true,
            level_labels: None,
            wants_assistant: false,
            purpose: lv.purpose(),
            cost_groups: all_cost_groups(),
            tender_data: project.tender_data_of(index),
            tender_mask: lv.oz_mask().pattern().to_string(),
        })
    }

    /// Puts a mask into the form (e.g. from the Gliederungs-Assistent).
    pub fn set_mask(&mut self, pattern: &str) {
        self.mask = pattern.to_string();
    }

    /// An empty LV with the form's mask and step, for the Gliederungs-Assistent.
    pub fn sample_lv(&self) -> Lv {
        let mask = OzMask::parse(self.mask.trim()).unwrap_or_else(|_| {
            OzMask::parse(OzMask::DEFAULT_PATTERN).expect("Default-OZ-Maske ist gültig")
        });
        let mut lv = Lv::new(self.title.trim(), mask);
        lv.set_position_step(self.step);
        if let Some(labels) = &self.level_labels {
            lv.set_level_labels(labels.clone());
        }
        lv
    }

    /// Takes mask, step and level labels the assistant set on `lv`.
    pub fn take_outline(&mut self, lv: &Lv) {
        self.mask = lv.oz_mask().pattern().to_string();
        self.step = lv.position_step();
        self.level_labels = Some(lv.level_labels().to_vec());
    }

    pub fn errors(&self) -> LvErrors {
        let number = self.number.trim();
        LvErrors {
            number: if number.is_empty() {
                Some("Bitte eine LV-Nummer angeben.".to_string())
            } else if self.other_numbers.iter().any(|other| other == number) {
                Some(format!(
                    "Die LV-Nummer „{number}“ gibt es im Projekt schon."
                ))
            } else {
                None
            },
            title: self
                .title
                .trim()
                .is_empty()
                .then(|| "Bitte eine Bezeichnung angeben.".to_string()),
            mask: match OzMask::parse(self.mask.trim()) {
                Ok(mask) => mask.step_problem(self.step),
                Err(error) => Some(error),
            },
            vat: parse_vat(&self.vat).err(),
            currency: self
                .currency
                .trim()
                .is_empty()
                .then(|| "Bitte eine Währung angeben (z. B. EUR).".to_string()),
            lump_sum: parse_lump_sum(&self.lump_sum).err(),
        }
    }

    /// Whether taking over renumbers the rows of the LV.
    pub fn renumbers(&self) -> bool {
        self.has_rows && (self.mask.trim() != self.original_mask || self.step != self.original_step)
    }

    /// The chosen Kostengruppen of a new Kosten-LV („420“ …).
    fn chosen_cost_groups(&self) -> Vec<&str> {
        self.cost_groups
            .iter()
            .filter(|(_, chosen)| *chosen)
            .map(|(code, _)| code.as_str())
            .collect()
    }

    /// A new LV from the form; a Kosten-LV with the mask of its Stufe gets the
    /// titles of the chosen Kostengruppen (DIN 276).
    pub fn build(&self) -> Result<Lv, String> {
        let mut lv = match self.purpose.cost_stage() {
            Some(stage)
                if self.mask.trim() == cost_lv_mask(stage).pattern()
                    && !self.chosen_cost_groups().is_empty() =>
            {
                cost_lv_template(
                    stage,
                    self.title.trim(),
                    &self.chosen_cost_groups(),
                    self.step,
                )
            }
            _ => Lv::new(
                self.title.trim(),
                OzMask::parse(OzMask::DEFAULT_PATTERN).expect("Default-OZ-Maske ist gültig"),
            ),
        };
        self.write(&mut lv)?;
        if let Some(labels) = &self.level_labels {
            lv.set_level_labels(labels.clone());
        }
        Ok(lv)
    }

    /// Writes the form into `lv`; true when anything changed.
    pub fn write(&self, lv: &mut Lv) -> Result<bool, String> {
        if !self.purpose.is_tender() && !self.tender_data.is_empty() {
            return Err(format!(
                "Das LV hat schon {} – es bleibt ein Ausschreibungs-LV. Für die Kostenermittlung \
                 ein neues LV anlegen.",
                self.tender_data
            ));
        }
        let errors = self.errors();
        if let Some(error) = [
            errors.number,
            errors.title,
            errors.mask,
            errors.vat,
            errors.currency,
            errors.lump_sum,
        ]
        .into_iter()
        .flatten()
        .next()
        {
            return Err(error);
        }
        let before = lv.clone();
        lv.set_purpose(self.purpose);
        let mask = OzMask::parse(self.mask.trim())?;
        lv.set_number(self.number.trim());
        lv.set_title(self.title.trim());
        lv.set_vat_percent(parse_vat(&self.vat)?);
        lv.set_currency(self.currency.trim().to_uppercase());
        lv.set_lump_sum_total(parse_lump_sum(&self.lump_sum)?);
        let structure_changed =
            mask.pattern() != lv.oz_mask().pattern() || self.step != lv.position_step();
        lv.set_position_step(self.step);
        lv.set_oz_mask(mask);
        if structure_changed && !lv.rows().is_empty() {
            lv.renumber();
        }
        Ok(*lv != before)
    }

    /// Draws the form; returns `(cancelled, confirmed)`.
    pub fn show(&mut self, ui: &mut egui::Ui, palette: &Palette) -> (bool, bool) {
        let title = match self.target {
            LvTarget::New => "Neues LV",
            LvTarget::Existing(_) => "Eigenschaften: LV",
        };
        widgets::title(
            ui,
            palette,
            title,
            if self.purpose.is_tender() {
                "Ein Projekt kann mehrere LVs haben (Lose, Gewerke) – jedes mit eigenen Angeboten und Preisspiegel."
            } else {
                "Kostenermittlung nach DIN 276 als Kurztext-LV: Menge × Kostenkennwert je Position, \
                 gegliedert nach Kostengruppen. Kein Ausschreibungs-LV – ohne Bieter, Angebote und Auftrag."
            },
        );
        let mut errors = self.errors();
        // Leere Pflichtfelder erst nach dem ersten „Anlegen“ rot zeigen.
        if self.error.is_none() {
            if self.title.trim().is_empty() {
                errors.title = None;
            }
            if self.number.trim().is_empty() {
                errors.number = None;
            }
        }
        let mut focus = std::mem::take(&mut self.focus);
        ui.label(RichText::new("Art").size(12.0).color(palette.text_dim));
        ui.add_space(3.0);
        let mut purpose = self.purpose;
        if form_kit::choice(ui, palette, &PURPOSES, &mut purpose) {
            self.set_purpose(purpose);
        }
        if !self.purpose.is_tender() && !self.tender_data.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Das LV hat schon {} – es bleibt ein Ausschreibungs-LV.",
                    self.tender_data
                ))
                .size(12.0)
                .color(palette.warn),
            );
        }
        ui.add_space(6.0);
        form_kit::split(
            ui,
            0.25,
            |ui| {
                let response = form_kit::field(
                    ui,
                    palette,
                    "LV-Nr.",
                    &mut self.number,
                    errors.number.as_deref(),
                );
                if std::mem::take(&mut focus) {
                    response.request_focus();
                }
            },
            |ui| {
                form_kit::field(
                    ui,
                    palette,
                    "Bezeichnung",
                    &mut self.title,
                    errors.title.as_deref(),
                );
            },
        );
        if self.target == LvTarget::New {
            form_kit::split(
                ui,
                0.72,
                |ui| {
                    form_kit::field(
                        ui,
                        palette,
                        "OZ-Maske",
                        &mut self.mask,
                        errors.mask.as_deref(),
                    );
                },
                |ui| {
                    ui.add_space(18.0);
                    if ui
                        .button("Gliederung …")
                        .on_hover_text(
                            "Stufen mit Bezeichnung und Stellen im Assistenten festlegen",
                        )
                        .clicked()
                    {
                        self.wants_assistant = true;
                    }
                },
            );
        } else {
            form_kit::field(
                ui,
                palette,
                "OZ-Maske",
                &mut self.mask,
                errors.mask.as_deref(),
            );
        }
        if let Ok(mask) = OzMask::parse(self.mask.trim()) {
            // A Kosten-LV numbers its titles with the Kostengruppe.
            let example = match self.purpose.cost_stage() {
                Some(stage)
                    if self.target == LvTarget::New
                        && mask.pattern() == cost_lv_mask(stage).pattern() =>
                {
                    let titles: &[u32] = match stage {
                        CostStage::Estimate => &[420],
                        CostStage::Calculation => &[420, 421],
                    };
                    format!(
                        "{}.{:04} (Titel = Kostengruppe)",
                        mask.format_group(titles),
                        self.step
                    )
                }
                _ => mask.example_with_step(self.step),
            };
            form_kit::hint(ui, palette, &format!("Beispiel: {example}"));
            ui.add_space(6.0);
        }
        if let (LvTarget::New, Some(stage)) = (self.target, self.purpose.cost_stage()) {
            self.cost_groups_ui(ui, palette, stage);
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
        form_kit::split(
            ui,
            0.5,
            |ui| {
                form_kit::field(
                    ui,
                    palette,
                    "Umsatzsteuer in %",
                    &mut self.vat,
                    errors.vat.as_deref(),
                );
            },
            |ui| {
                form_kit::field(
                    ui,
                    palette,
                    "Währung",
                    &mut self.currency,
                    errors.currency.as_deref(),
                );
            },
        );
        if self.target != LvTarget::New {
            form_kit::field(
                ui,
                palette,
                "Pauschalsumme netto (leer = Summe der Positionen)",
                &mut self.lump_sum,
                errors.lump_sum.as_deref(),
            );
            form_kit::hint(
                ui,
                palette,
                "Ersetzt die Summe der Positionen als LV-Summe (GAEB TotalLSUM); die Titel behalten ihre Summen.",
            );
            ui.add_space(6.0);
        }
        if self.renumbers() {
            ui.label(
                RichText::new(
                    "Maske oder Schrittweite geändert: die Zeilen werden neu nummeriert.",
                )
                .size(12.0)
                .color(palette.warn),
            );
            ui.add_space(6.0);
        }
        widgets::error_line(ui, palette, self.error.as_deref());
        let label = match self.target {
            LvTarget::New => "Anlegen",
            LvTarget::Existing(_) => "Übernehmen",
        };
        widgets::dialog_buttons(ui, palette, label)
    }
}

impl LvPropertiesForm {
    /// Kostengruppen of the 2nd level of KG 400 that get a title.
    fn cost_groups_ui(&mut self, ui: &mut egui::Ui, palette: &Palette, stage: CostStage) {
        ui.label(
            RichText::new("Titel nach Kostengruppen (DIN 276, KG 400)")
                .size(12.0)
                .color(palette.text_dim),
        );
        ui.add_space(3.0);
        let names = kg400_groups();
        egui::Grid::new("lv_cost_groups")
            .num_columns(3)
            .spacing([12.0, 2.0])
            .show(ui, |ui| {
                for (index, (code, chosen)) in self.cost_groups.iter_mut().enumerate() {
                    let name = names
                        .iter()
                        .find(|(known, _)| *known == code.as_str())
                        .map(|(_, name)| *name)
                        .unwrap_or("");
                    ui.checkbox(chosen, code.as_str())
                        .on_hover_text(format!("KG {code} {name}"));
                    if index % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        form_kit::hint(
            ui,
            palette,
            match stage {
                CostStage::Estimate => {
                    "Je Kostengruppe der 2. Ebene ein Titel mit der KG als OZ (z. B. 420); \
                     die Positionen übernehmen die KG des Titels."
                }
                CostStage::Calculation => {
                    "Je Kostengruppe der 2. Ebene ein Titel, darunter je KG der 3. Ebene ein \
                     Untertitel (z. B. 420.421); leere Untertitel lassen sich löschen."
                }
            },
        );
        ui.add_space(6.0);
    }
}

/// The next free LV number: one more than the highest numeric one.
pub fn next_lv_number(project: &Project) -> String {
    let highest = project
        .lvs()
        .filter_map(|lv| lv.number().trim().parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let mut next = highest + 1;
    let used: Vec<String> = project
        .lvs()
        .map(|lv| lv.number().trim().to_string())
        .collect();
    while used.contains(&next.to_string()) {
        next += 1;
    }
    next.to_string()
}

/// „LV 2 – Gebäudeautomation“ for lists and the header.
pub fn lv_label(lv: &Lv) -> String {
    match (lv.number().trim(), lv.title().trim()) {
        ("", title) => title.to_string(),
        (number, "") => format!("LV {number}"),
        (number, title) => format!("LV {number} – {title}"),
    }
}

/// [`lv_label`] with the Stufe of a Kosten-LV, unless the title says it:
/// „LV 7 – Heizung (Kostenberechnung)“ (the LV table names it in the head row).
pub fn lv_label_with_stage(lv: &Lv) -> String {
    let label = lv_label(lv);
    match lv.purpose().cost_stage() {
        Some(stage) if !label.contains(stage.label()) => format!("{label} ({})", stage.label()),
        _ => label,
    }
}

fn parse_vat(text: &str) -> Result<Decimal, String> {
    let text = text.trim().trim_end_matches('%').trim();
    let value = parse_decimal_de(text).map_err(|_| format!("„{text}“ ist keine Zahl."))?;
    if value < Decimal::ZERO || value > Decimal::ONE_HUNDRED {
        return Err("Die Umsatzsteuer liegt zwischen 0 und 100 %.".to_string());
    }
    Ok(value)
}

/// Pauschalsumme: empty = none, otherwise a German amount ≥ 0.
fn parse_lump_sum(text: &str) -> Result<Option<proava_domain::Money>, String> {
    let text = text.trim().trim_end_matches('€').trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value = parse_decimal_de(text).map_err(|_| format!("„{text}“ ist kein Betrag."))?;
    if value.is_sign_negative() {
        return Err("Die Pauschalsumme darf nicht negativ sein.".to_string());
    }
    Ok(Some(proava_domain::Money::from_decimal(value)))
}

fn vat_text(vat: Decimal) -> String {
    format_decimal_de(vat, 2)
        .trim_end_matches('0')
        .trim_end_matches(',')
        .to_string()
}

#[cfg(test)]
mod tests {
    use proava_domain::{Decimal, Lv, LvGroup, OzMask, Project, ProjectName};

    use super::{lv_label, next_lv_number, LvPropertiesForm};

    fn project() -> Project {
        let mut lv = Lv::new("Gebäudeautomation", OzMask::parse("11.22.PPPP").unwrap());
        lv.set_number("1");
        lv.set_vat_percent(Decimal::from(7));
        Project::new(ProjectName::parse("Schule").unwrap(), "2026-11-17", lv)
    }

    #[test]
    fn lump_sum_is_set_and_cleared() {
        let mut project = project();
        let mut form = LvPropertiesForm::existing(&project, 0).unwrap();
        assert_eq!(form.lump_sum, "");
        form.lump_sum = "1.460.000,00".to_string();
        assert!(form.write(project.lv_mut()).unwrap());
        assert_eq!(
            project.lv().lump_sum_total(),
            Some(proava_domain::Money::from_cents(146_000_000))
        );
        let mut form = LvPropertiesForm::existing(&project, 0).unwrap();
        assert_eq!(form.lump_sum, "1.460.000,00");
        form.lump_sum = "-5".to_string();
        assert!(form.errors().lump_sum.is_some());
        form.lump_sum.clear();
        assert!(form.write(project.lv_mut()).unwrap());
        assert_eq!(project.lv().lump_sum_total(), None);
    }

    #[test]
    fn new_lv_takes_the_active_one_as_template() {
        let project = project();
        let mut form = LvPropertiesForm::new_lv(&project);
        assert_eq!(form.number, "2");
        assert_eq!(form.mask, "11.22.PPPP");
        assert_eq!(form.vat, "7");
        assert!(form.errors().title.is_some());
        form.title = "Elektro".into();
        let lv = form.build().unwrap();
        assert_eq!(lv_label(&lv), "LV 2 – Elektro");
        assert_eq!(lv.oz_mask().pattern(), "11.22.PPPP");
        let mut outline = form.sample_lv();
        outline.set_oz_mask(OzMask::parse("1.22.PPP").unwrap());
        outline.set_level_labels(vec!["Los".into(), "Titel".into()]);
        outline.set_position_step(1);
        form.take_outline(&outline);
        let lv = form.build().unwrap();
        assert_eq!(lv.oz_mask().pattern(), "1.22.PPP");
        assert_eq!(lv.position_step(), 1);
        assert_eq!(lv.level_labels(), ["Los".to_string(), "Titel".to_string()]);
        form.number = "1".into();
        assert!(form.errors().number.unwrap().contains("schon"));
        assert!(form.build().is_err());
    }

    #[test]
    fn existing_lv_is_changed_and_renumbered() {
        let mut project = project();
        project.lv_mut().push_group(LvGroup::new(0, "01", "Titel"));
        let mut form = LvPropertiesForm::existing(&project, 0).unwrap();
        assert!(!form.renumbers());
        assert!(!form.write(project.lv_mut()).unwrap());
        form.mask = "111.PPPP".into();
        assert!(form.renumbers());
        form.vat = "19".into();
        form.currency = "eur".into();
        assert!(form.write(project.lv_mut()).unwrap());
        assert_eq!(project.lv().oz_mask().pattern(), "111.PPPP");
        assert_eq!(project.lv().currency(), "EUR");
        form.mask = "unsinn?".into();
        assert!(form.write(project.lv_mut()).is_err());
    }

    #[test]
    fn next_number_skips_used_ones() {
        let mut project = project();
        let mut second = Lv::new("B", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        second.set_number("A");
        project.add_lv(second);
        assert_eq!(next_lv_number(&project), "2");
    }
}
