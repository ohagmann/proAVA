//! Projektstammdaten → Deckblatt of the LV printouts and the following page
//! „Vertragsbedingungen“. Order and labels of the facts are set in
//! `proava-print/templates/cover.typ`; here only the values by key.

use proava_domain::{format_decimal_de, Address, Decimal, Project};
use proava_print::{CoverBlocks, CoverSection, CoverStyle, PrintCover};

/// The Deckblatt for the active LV in `style` with the chosen `blocks`.
/// Empty fields are left out by the template.
pub fn project_cover(project: &Project, style: CoverStyle, blocks: CoverBlocks) -> PrintCover {
    let info = project.info();
    let lv = project.lv();
    let lv_label = match (lv.number().trim(), lv.title().trim()) {
        ("", title) => title.to_string(),
        (number, title) => format!("LV {number} {title}"),
    };
    let project_label = if info.label.trim().is_empty() {
        project.name().as_str().to_string()
    } else {
        info.label.trim().to_string()
    };
    let execution = match (info.execution_start.trim(), info.execution_end.trim()) {
        ("", "") => String::new(),
        (start, "") => format!("ab {start}"),
        ("", end) => format!("bis {end}"),
        (start, end) => format!("{start} bis {end}"),
    };
    let facts = [
        ("projektnummer", info.short_name.clone()),
        ("baustelle", info.site.one_line()),
        ("vergabeart", info.award_procedure.clone()),
        ("vergabenummer", info.award_number.clone()),
        ("abgabetermin", info.submission_deadline.clone()),
        ("abgabeort", info.submission_place.clone()),
        ("bindefrist", info.binding_period_end.clone()),
        ("ausfuehrung", execution),
        ("weitere_fristen", info.execution_note.clone()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.trim().to_string()))
    .collect();
    // Without an own Auftraggeber (Vergabestelle) the template labels the
    // owner „Bauherr / Auftraggeber“.
    let mut addresses = vec![("bauherr".to_string(), lines(&info.owner))];
    if info.client_differs() {
        addresses.push(("auftraggeber".to_string(), lines(info.client())));
    }
    addresses.push(("planer".to_string(), lines(&info.planner)));

    let terms = &info.terms;
    let mut security = Section::new("Sicherheiten");
    if let Some(percent) = terms.performance_security_percent {
        security.add(
            "Sicherheit für die Vertragserfüllung",
            &format!("{} der Auftragssumme", percent_text(percent)),
        );
    }
    if let Some(percent) = terms.warranty_security_percent {
        security.add(
            "Sicherheit für Mängelansprüche",
            &format!("{} der Abrechnungssumme", percent_text(percent)),
        );
    }
    let mut payment = Section::new("Zahlung und Abzüge");
    if let Some(percent) = terms.cash_discount_percent {
        let text = match terms.cash_discount_days {
            Some(days) => format!(
                "{} bei Zahlung innerhalb von {days} Tagen",
                percent_text(percent)
            ),
            None => percent_text(percent),
        };
        payment.add("Skonto", &text);
    }
    let deductions: Vec<_> = terms
        .deductions
        .iter()
        .filter(|deduction| !deduction.label.trim().is_empty() && !deduction.percent.is_zero())
        .collect();
    if !deductions.is_empty() {
        let mut lines: Vec<String> = deductions
            .iter()
            .map(|deduction| {
                format!(
                    "{} {}",
                    deduction.label.trim(),
                    percent_text(deduction.percent)
                )
            })
            .collect();
        if deductions.len() > 1 {
            lines.push(format!(
                "zusammen {}",
                percent_text(terms.deduction_percent())
            ));
        }
        payment.add("Abzüge / Umlagen", &lines.join("\n"));
    }
    let mut others = Section::new("Weitere Beteiligte");
    for contact in &info.contacts {
        let role = if contact.role.trim().is_empty() {
            "Weitere Adresse"
        } else {
            contact.role.trim()
        };
        others.add(role, &lines(&contact.address).join("\n"));
    }

    let mut notes = Vec::new();
    if !terms.payment_terms.trim().is_empty() {
        notes.push((
            "Zahlungsbedingungen".to_string(),
            terms.payment_terms.trim().to_string(),
        ));
    }
    if !terms.notes.trim().is_empty() {
        notes.push((
            "Weitere Vereinbarungen".to_string(),
            terms.notes.trim().to_string(),
        ));
    }

    PrintCover {
        style,
        blocks,
        lv: lv_label,
        project: project_label,
        measure: info.description.trim().to_string(),
        facts,
        addresses,
        sections: [security, payment, others]
            .into_iter()
            .map(Section::finish)
            .collect(),
        notes,
    }
}

struct Section(CoverSection);

impl Section {
    fn new(heading: &str) -> Self {
        Self(CoverSection {
            heading: heading.to_string(),
            rows: Vec::new(),
        })
    }

    fn add(&mut self, label: &str, value: &str) {
        let value = value.trim();
        if !value.is_empty() {
            self.0.rows.push((label.to_string(), value.to_string()));
        }
    }

    fn finish(self) -> CoverSection {
        self.0
    }
}

/// Address as lines: name, Zusatz, street, PLZ Ort, phone/e-mail.
fn lines(address: &Address) -> Vec<String> {
    let city = format!("{} {}", address.postal_code.trim(), address.city.trim());
    let contact = [address.phone.trim(), address.email.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    [
        address.name1.trim(),
        address.name2.trim(),
        address.street.trim(),
        city.trim(),
        contact.as_str(),
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .map(str::to_string)
    .collect()
}

fn percent_text(value: Decimal) -> String {
    let text = format_decimal_de(value, 3);
    format!("{} %", text.trim_end_matches('0').trim_end_matches(','))
}

#[cfg(test)]
mod tests {
    use proava_domain::{
        Address, Contact, ContractTerms, Lv, OzMask, Project, ProjectInfo, ProjectName,
    };

    use proava_print::{CoverBlocks, CoverStyle};

    use super::project_cover;

    fn cover(project: &Project) -> proava_print::PrintCover {
        project_cover(project, CoverStyle::Standard, CoverBlocks::default())
    }

    #[test]
    fn cover_shows_master_data_and_leaves_out_empty_fields() {
        let mut lv = Lv::new(
            "Gebäudeautomation",
            OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
        );
        lv.set_number("1");
        let mut project = Project::new(ProjectName::parse("Schule").unwrap(), "2026-11-17", lv);
        let empty = cover(&project);
        assert_eq!(
            empty
                .sections
                .iter()
                .map(|section| section.rows.len())
                .sum::<usize>(),
            0
        );
        assert_eq!(empty.lv, "LV 1 Gebäudeautomation");
        assert_eq!(
            empty.project, "Schule",
            "ohne Projektbezeichnung der Projektname"
        );
        assert!(empty.facts.iter().all(|(_, value)| value.is_empty()));

        let mut terms = ContractTerms {
            performance_security_percent: Some("5".parse().unwrap()),
            cash_discount_percent: Some("2".parse().unwrap()),
            cash_discount_days: Some(14),
            deductions: ContractTerms::common_deductions(),
            payment_terms: "Abschläge nach § 16 VOB/B".into(),
            ..ContractTerms::default()
        };
        terms.deductions[0].percent = "0.3".parse().unwrap();
        terms.deductions[1].percent = "0.5".parse().unwrap();
        *project.info_mut() = ProjectInfo {
            label: "Neubau Schulzentrum".into(),
            owner: Address {
                name1: "Stadt Musterstadt".into(),
                street: "Rathausplatz 1".into(),
                postal_code: "12345".into(),
                city: "Musterstadt".into(),
                ..Address::default()
            },
            contacts: vec![Contact {
                role: "Architekt".into(),
                address: Address::named("Büro Plan"),
            }],
            award_procedure: "Öffentliche Ausschreibung".into(),
            submission_deadline: "15.12.2026, 10:00 Uhr".into(),
            execution_start: "01.03.2027".into(),
            execution_end: "30.11.2027".into(),
            terms,
            ..ProjectInfo::default()
        };
        let cover = cover(&project);
        let fact = |key: &str| {
            cover
                .facts
                .iter()
                .find(|(known, _)| known == key)
                .map(|(_, value)| value.clone())
                .unwrap()
        };
        assert_eq!(fact("ausfuehrung"), "01.03.2027 bis 30.11.2027");
        assert_eq!(fact("abgabetermin"), "15.12.2026, 10:00 Uhr");
        assert_eq!(
            cover.addresses[0],
            (
                "bauherr".to_string(),
                vec![
                    "Stadt Musterstadt".to_string(),
                    "Rathausplatz 1".into(),
                    "12345 Musterstadt".into()
                ]
            )
        );
        assert_eq!(cover.project, "Neubau Schulzentrum");
        let all: Vec<(String, String)> = cover
            .sections
            .iter()
            .flat_map(|section| section.rows.clone())
            .collect();
        let value = |label: &str| {
            all.iter()
                .find(|(known, _)| known == label)
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("{label} fehlt: {all:?}"))
        };
        assert_eq!(value("Architekt"), "Büro Plan");
        assert_eq!(value("Skonto"), "2 % bei Zahlung innerhalb von 14 Tagen");
        assert!(value("Abzüge / Umlagen").starts_with("Bauwesenversicherung 0,3 %"));
        assert_eq!(
            value("Abzüge / Umlagen"),
            "Bauwesenversicherung 0,3 %\nBaustrom 0,5 %\nzusammen 0,8 %",
            "Abzüge mit 0 % fallen weg"
        );
        assert_eq!(cover.notes.len(), 1);

        // An own Auftraggeber: a block of its own (the template labels the
        // owner „Bauherr“ then, without one „Bauherr / Auftraggeber“).
        assert_eq!(cover.addresses.len(), 2, "Bauherr und Planer");
        project.info_mut().own_client = Some(Address::named("Gebäudemanagement Musterstadt"));
        let cover = super::project_cover(&project, CoverStyle::Letterhead, CoverBlocks::default());
        assert_eq!(cover.style, CoverStyle::Letterhead);
        assert_eq!(
            cover.addresses[1],
            (
                "auftraggeber".to_string(),
                vec!["Gebäudemanagement Musterstadt".to_string()]
            )
        );
    }
}
