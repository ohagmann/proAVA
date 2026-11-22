//! Projektvorlagen: ein normales Projekt, aus dem neue Projekte entstehen.
//!
//! Eine Vorlage bringt mit: die LVs mit OZ-Maske, Schrittweite, Gliederung
//! (Titel mit Bezeichnung und Kostengruppe), Vorbemerkungen und Hinweise,
//! Ausführungsbeschreibungen, auf Wunsch die Positionen, die Stammdaten-
//! Vorgaben (Planer, Vergabeart, Vertragsbedingungen mit Abzügen) und die
//! Briefvorlagen und die Kostenermittlungen (Anlagenlisten nur mit den
//! Positionen). Nie mit kommen Angebote, Bieterverzeichnis, Auftrag,
//! Nachträge und Abrechnung; Positionen beginnen wieder „offen“.

use rust_decimal::Decimal;

use crate::cost_estimate::PointRow;
use crate::lv::{Lv, LvGroup, LvRow, Note};
use crate::oz_mask::OzMask;
use crate::performance::PerformanceDescription;
use crate::project::{ContractTerms, ProjectInfo};
use crate::project_name::ProjectName;
use crate::rich_text::RichText;
use crate::{EditStatus, Project};

/// Name of the template that comes with proAVA.
pub const GA_TEMPLATE_NAME: &str = "GA nach DIN 276 (481–489)";

/// OZ-Maske the GA template brings along: Los/Abschnitt (01), Kostengruppe
/// as title number (01.481), position with four digits and index
/// (01.481.0010.1). The dialog „Neues Projekt“ offers it and lets it be
/// changed; the settings mask does not apply to this template.
pub const GA_TEMPLATE_MASK: &str = "11.222.PPPPI";

/// Titles of the built-in template: Kostengruppe and label.
pub const GA_TITLES: [(&str, &str); 6] = [
    ("481", "Automationseinrichtungen"),
    ("482", "Schaltschränke"),
    ("483", "Management- und Bedieneinrichtungen"),
    ("484", "Kabel und Leitungen"),
    ("485", "Netzwerke"),
    ("489", "Sonstiges"),
];

/// A new project from a template: everything the template holds except
/// bids, contract and billing; `keep_positions = false` leaves only the
/// outline, hints and performance descriptions.
pub fn project_from_template(
    template: &Project,
    name: ProjectName,
    created_iso: &str,
    keep_positions: bool,
) -> Project {
    rebuild(
        template,
        name,
        created_iso,
        keep_positions,
        template.info().clone(),
    )
}

/// A template from a project („Als Vorlage speichern …“): like
/// [`project_from_template`], and the facts of this one project stay
/// behind (Bauherr, Auftraggeber, Baustelle, weitere Adressen,
/// Bezeichnung, Vergabenummer, Termine).
pub fn template_from_project(
    project: &Project,
    name: ProjectName,
    created_iso: &str,
    keep_positions: bool,
) -> Project {
    rebuild(
        project,
        name,
        created_iso,
        keep_positions,
        general_info(project.info()),
    )
}

/// What a template holds, for a line in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TemplateSummary {
    pub lvs: usize,
    pub groups: usize,
    pub notes: usize,
    pub performances: usize,
    pub positions: usize,
    /// Kostenschätzungen und -berechnungen.
    pub cost_estimates: usize,
}

pub fn summary(project: &Project) -> TemplateSummary {
    let mut summary = TemplateSummary {
        lvs: project.lv_count(),
        cost_estimates: project.cost_estimates().len(),
        ..TemplateSummary::default()
    };
    for lv in project.lvs() {
        summary.groups += lv.group_count();
        summary.notes += lv.note_count();
        summary.performances += lv.performance_count();
        summary.positions += lv.position_count();
    }
    summary
}

impl TemplateSummary {
    /// „6 Titel · 1 Hinweis · 1 Ausführungsbeschreibung · keine Positionen“
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.lvs > 1 {
            parts.push(format!("{} LVs", self.lvs));
        }
        parts.push(count(self.groups, "Titel", "Titel"));
        parts.push(count(self.notes, "Hinweis", "Hinweise"));
        parts.push(count(
            self.performances,
            "Ausführungsbeschreibung",
            "Ausführungsbeschreibungen",
        ));
        parts.push(if self.positions == 0 {
            "keine Positionen".to_string()
        } else {
            count(self.positions, "Position", "Positionen")
        });
        if self.cost_estimates > 0 {
            parts.push(count(
                self.cost_estimates,
                "Kostenermittlung",
                "Kostenermittlungen",
            ));
        }
        parts.join(" · ")
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The template that comes with proAVA: one LV „Gebäudeautomation“ with the
/// titles 481–489 after DIN 276, a Vorbemerkung, a general performance
/// description and the usual deductions at 0 %. The mask is usually
/// [`ga_template_mask`] (the dialog lets the user change it), step and VAT
/// come from the settings. The titles sit on the first level with at least
/// three digits (OZ = Kostengruppe, e.g. `11.222.PPPPI` → 01.481), otherwise
/// on the first level, numbered 01, 02 …
pub fn ga_template(mask: OzMask, step: u32, vat_percent: Decimal, created_iso: &str) -> Project {
    let mut lv = Lv::new("Gebäudeautomation", mask.clone());
    lv.set_number("480");
    lv.set_position_step(step);
    lv.set_vat_percent(vat_percent);

    let mut preliminary = Note::rich(RichText::plain(
        "Die Gliederung folgt DIN 276 (Kostengruppen 481 bis 489).\n\
         Die Leistungen umfassen Lieferung, Montage, Programmierung, Inbetriebnahme, \
         Einweisung und Dokumentation, soweit die Positionen nichts anderes sagen.\n\
         Diese Vorbemerkungen sind ein Vorschlag und an das Projekt anzupassen.",
    ));
    preliminary.set_heading("Vorbemerkungen Gebäudeautomation");
    preliminary.set_preliminary(true);
    lv.push_note(preliminary);

    let performance = PerformanceDescription::new(
        "1",
        "Allgemeine Anforderungen an GA-Komponenten",
        RichText::plain(
            "Alle Komponenten sind für den Dauerbetrieb geeignet und tragen die \
             CE-Kennzeichnung.\n\
             Zu jedem Gerät gehören Datenblatt und Bedienungsanleitung in deutscher Sprache.\n\
             Kommunikationsfähige Geräte nutzen offene Protokolle (z. B. BACnet, Modbus).",
        ),
    );
    lv.push_row(LvRow::Performance(performance));

    let levels = mask.level_count();
    let kg_level = cost_group_level(&mask).unwrap_or(0);
    let mut parents = Vec::new();
    for level in 0..kg_level.min(levels.saturating_sub(1)) {
        parents.push(1);
        lv.push_group(LvGroup::new(
            level,
            mask.format_group(&parents),
            "Gebäudeautomation",
        ));
    }
    let as_code = cost_group_level(&mask).is_some();
    for (index, (code, label)) in GA_TITLES.iter().enumerate() {
        let number = if as_code {
            code.parse::<u32>().unwrap_or(index as u32 + 1)
        } else {
            index as u32 + 1
        };
        let mut levels_of = parents.clone();
        levels_of.push(number);
        let mut group = LvGroup::new(parents.len(), mask.format_group(&levels_of), *label);
        group.set_cost_group(Some((*code).to_string()));
        lv.push_group(group);
    }

    let name = ProjectName::parse(GA_TEMPLATE_NAME).expect("Vorlagenname ist gültig");
    let mut project = Project::new(name, created_iso, lv);
    project.info_mut().terms = ContractTerms {
        deductions: ContractTerms::common_deductions(),
        ..ContractTerms::default()
    };
    project
}

/// [`GA_TEMPLATE_MASK`] as a mask.
pub fn ga_template_mask() -> OzMask {
    OzMask::parse(GA_TEMPLATE_MASK).expect("Maske der GA-Vorlage ist gültig")
}

/// The level that takes the cost group (481 …) as its number in the GA
/// template: the first level of digits with at least three places. None when
/// the mask has none – then the titles are numbered 01, 02 … and carry the
/// KG only as the title's cost group.
pub fn cost_group_level(mask: &OzMask) -> Option<usize> {
    (0..mask.level_count())
        .find(|&level| mask.level_width(level) >= 3 && !mask.level_is_letters(level))
}

fn rebuild(
    source: &Project,
    name: ProjectName,
    created_iso: &str,
    keep_positions: bool,
    info: ProjectInfo,
) -> Project {
    let mut lvs = source.lvs().map(|lv| template_lv(lv, keep_positions));
    let first = lvs.next().expect("ein Projekt hat immer mindestens ein LV");
    let mut project = Project::new(name, created_iso, first);
    for lv in lvs {
        project.add_lv(lv);
    }
    project.set_active_lv(source.active_lv());
    *project.info_mut() = info;
    for image in source.images() {
        project.add_asset_copy(image);
    }
    project.replace_letter_templates(source.letter_templates().to_vec());
    // Kostenermittlungen kommen mit DP-Arten, Preisen, Bauteilen und
    // Aufteilung mit; die Anlagenlisten nur zusammen mit den Positionen.
    for estimate in source.cost_estimates() {
        let mut estimate = estimate.clone();
        if !keep_positions {
            // One empty line per Bauteil to start typing.
            let blank = PointRow {
                counts: vec![0; estimate.kinds.len()],
                ..PointRow::default()
            };
            for section in &mut estimate.sections {
                section.rows = vec![blank.clone()];
            }
        }
        project.add_cost_estimate(estimate);
    }
    project.remove_unused_images();
    project
}

fn template_lv(source: &Lv, keep_positions: bool) -> Lv {
    let mut lv = source.clone();
    lv.set_request_exported(None);
    if !keep_positions {
        for index in (0..lv.rows().len()).rev() {
            if lv.rows()[index].is_position() {
                lv.remove_row(index);
            }
        }
    }
    for index in 0..lv.rows().len() {
        if let Some(LvRow::Position(position)) = lv.row_mut(index) {
            position.set_status(EditStatus::Open);
        }
    }
    lv
}

/// Stammdaten that hold for more than one project.
fn general_info(info: &ProjectInfo) -> ProjectInfo {
    ProjectInfo {
        planner: info.planner.clone(),
        award_procedure: info.award_procedure.clone(),
        submission_place: info.submission_place.clone(),
        terms: info.terms.clone(),
        ..ProjectInfo::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bid::{Address, Bid};
    use crate::position::Position;

    fn mask(pattern: &str) -> OzMask {
        OzMask::parse(pattern).unwrap()
    }

    fn titles(project: &Project) -> Vec<(String, String, Option<String>)> {
        project
            .lv()
            .rows()
            .iter()
            .filter_map(LvRow::as_group)
            .map(|group| {
                (
                    group.ordinal().to_string(),
                    group.short_text().to_string(),
                    group.cost_group().map(str::to_string),
                )
            })
            .collect()
    }

    #[test]
    fn ga_template_numbers_the_titles_after_the_mask() {
        let flat = ga_template(mask("11.22.PPPPI"), 10, Decimal::from(19), "2026-11-17");
        let rows = titles(&flat);
        assert_eq!(rows.len(), 6);
        assert_eq!(
            rows[0],
            (
                "01".into(),
                "Automationseinrichtungen".into(),
                Some("481".into())
            )
        );
        assert_eq!(
            rows[5],
            ("06".into(), "Sonstiges".into(), Some("489".into()))
        );
        assert_eq!(flat.lv().position_step(), 10);
        assert_eq!(flat.lv().number(), "480");
        assert!(matches!(&flat.lv().rows()[0], LvRow::Note(note) if note.is_preliminary()));
        assert_eq!(flat.lv().performance_count(), 1);
        assert_eq!(flat.info().terms.deductions.len(), 4);

        let deep = ga_template(mask("11.222.33.PPPP"), 10, Decimal::from(19), "2026-11-17");
        let rows = titles(&deep);
        assert_eq!(rows[0], ("01".into(), "Gebäudeautomation".into(), None));
        assert_eq!(rows[1].0, "01.481");
        assert_eq!(rows[6].0, "01.489");
        assert!(rows[1..]
            .iter()
            .all(|(ordinal, _, _)| deep.lv().oz_mask().fits_group(ordinal)));

        let kg_first = ga_template(mask("111.22.PPPP"), 1, Decimal::from(19), "2026-11-17");
        assert_eq!(titles(&kg_first)[2].0, "483");
    }

    #[test]
    fn ga_template_brings_its_own_mask() {
        assert_eq!(GA_TEMPLATE_MASK, "11.222.PPPPI");
        let mut ga = ga_template(ga_template_mask(), 10, Decimal::from(19), "2026-11-18");
        assert_eq!(ga.lv().oz_mask().pattern(), "11.222.PPPPI");
        let rows = titles(&ga);
        assert_eq!(
            rows[0],
            ("01".into(), "Gebäudeautomation".into(), None),
            "Los/Abschnitt"
        );
        let expected = [
            ("01.481", "Automationseinrichtungen", "481"),
            ("01.482", "Schaltschränke", "482"),
            ("01.483", "Management- und Bedieneinrichtungen", "483"),
            ("01.484", "Kabel und Leitungen", "484"),
            ("01.485", "Netzwerke", "485"),
            ("01.489", "Sonstiges", "489"),
        ];
        let got: Vec<(String, String, Option<String>)> = rows[1..].to_vec();
        let want: Vec<(String, String, Option<String>)> = expected
            .iter()
            .map(|(oz, label, kg)| ((*oz).into(), (*label).into(), Some((*kg).into())))
            .collect();
        assert_eq!(got, want);

        // A position below 01.481 gets four digits and the index.
        let below = ga
            .lv()
            .rows()
            .iter()
            .position(|row| row.ordinal() == "01.481")
            .unwrap()
            + 1;
        ga.lv_mut()
            .insert_numbered(
                below,
                LvRow::Position(Position::new("", "DDC-Station", Decimal::ONE, "St", None)),
            )
            .unwrap();
        let position = ga.lv().positions().next().unwrap();
        assert!(
            position.ordinal().starts_with("01.481.0010"),
            "{}",
            position.ordinal()
        );
        assert!(
            ga.lv().oz_mask().fits_position(position.ordinal()),
            "{}",
            position.ordinal()
        );
    }

    #[test]
    fn cost_group_level_needs_three_digits() {
        assert_eq!(cost_group_level(&mask("11.22.PPPPI")), None);
        assert_eq!(cost_group_level(&mask("11.222.PPPPI")), Some(1));
        assert_eq!(cost_group_level(&mask("11.222.33.PPPP")), Some(1));
        assert_eq!(cost_group_level(&mask("111.22.PPPP")), Some(0));
        let ga = ga_template(mask("11.222.PPPPI"), 10, Decimal::from(19), "2026-11-18");
        assert_eq!(titles(&ga)[1].0, "01.481");
    }

    fn sample_project() -> Project {
        let ga = ga_template(mask("11.22.PPPPI"), 10, Decimal::from(19), "2026-01-01");
        let name = ProjectName::parse("Schule").unwrap();
        let mut project = project_from_template(&ga, name, "2026-01-01", true);
        let mut position = Position::new("01.0010", "DDC-Station", Decimal::ONE, "St", None);
        position.set_status(EditStatus::Done);
        project.lv_mut().push_position(position);
        project
            .lv_mut()
            .set_request_exported(Some("2026-02-01T10:00:00".into()));
        project.info_mut().owner = Address::named("Stadt Musterstadt");
        project.info_mut().planner = Address::named("Planungsbüro");
        project.info_mut().award_number = "V-12".into();
        project.add_bid(Bid::new(Address::named("Bieter A"), "a.x84"));
        project
    }

    #[test]
    fn a_template_keeps_the_outline_and_drops_the_project_facts() {
        let project = sample_project();
        let template = template_from_project(
            &project,
            ProjectName::parse("GA Schulen").unwrap(),
            "2026-11-17",
            false,
        );
        assert_eq!(template.name().as_str(), "GA Schulen");
        assert_eq!(template.lv().position_count(), 0);
        assert_eq!(template.lv().group_count(), 6);
        assert_eq!(template.lv().performance_count(), 1);
        assert!(template.bids().is_empty());
        assert_eq!(template.info().planner.name1, "Planungsbüro");
        assert_eq!(template.info().owner, Address::default());
        assert!(template.info().award_number.is_empty());
        assert_eq!(template.lv().request_exported(), None);
        assert_eq!(template.lv().oz_mask().pattern(), "11.22.PPPPI");
    }

    #[test]
    fn a_project_from_a_template_takes_everything_but_the_offers() {
        let template = sample_project();
        let project = project_from_template(
            &template,
            ProjectName::parse("Neubau").unwrap(),
            "2026-11-17",
            true,
        );
        assert_eq!(project.name().as_str(), "Neubau");
        assert_eq!(project.created_iso(), "2026-11-17");
        assert_eq!(project.lv().position_count(), 1);
        let position = project.lv().positions().next().unwrap();
        assert_eq!(
            position.status(),
            EditStatus::Open,
            "Positionen beginnen offen"
        );
        assert!(project.bids().is_empty());
        assert_eq!(
            project.info().owner.name1,
            "Stadt Musterstadt",
            "die Vorlage gilt, wie sie ist"
        );
        assert_eq!(summary(&project).positions, 1);
        assert_eq!(
            summary(&template).describe(),
            "6 Titel · 1 Hinweis · 1 Ausführungsbeschreibung · 1 Position"
        );
    }
}
