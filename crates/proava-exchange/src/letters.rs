//! Schriftverkehr: template + Bieterverzeichnis → letters for print.

use proava_domain::{
    letter_values, release_letter_values, Address, InviteeId, InvoiceId, LetterKind,
    LetterTemplate, Project,
};
use proava_print::{PrintLetter, PrintLetters};

use crate::to_print::blocks;

/// Letters of `kind` for the invited bidders `recipients` of the active LV.
pub fn letters_to_print(
    project: &Project,
    kind: LetterKind,
    recipients: &[InviteeId],
    date: &str,
) -> Result<PrintLetters, String> {
    if kind.to_client() {
        return release_letter_to_print(project, None, date);
    }
    let template = checked_template(project, kind)?;
    let mut letters = Vec::new();
    for id in recipients {
        let invitee = project
            .invitees()
            .iter()
            .find(|invitee| invitee.id == *id)
            .ok_or_else(|| format!("Den Bieter #{} gibt es im Bieterverzeichnis nicht.", id.0))?;
        let values = letter_values(project, invitee, date);
        let recipient = address_lines(&invitee.address, &invitee.contact_person);
        letters.push(letter(project, &template, &values, recipient, date, false));
    }
    if letters.is_empty() {
        return Err(format!("Kein Empfänger für „{}“.", kind.label()));
    }
    Ok(PrintLetters {
        title: format!("{} – {}", kind.label(), project.name().as_str()),
        logo: None,
        letters,
        frame: Default::default(),
        look: Default::default(),
    })
}

/// The letter to the client (Auftraggeber, else Bauherr) about the release
/// of `invoice` (None = the last one) of the active LV.
pub fn release_letter_to_print(
    project: &Project,
    invoice: Option<InvoiceId>,
    date: &str,
) -> Result<PrintLetters, String> {
    let kind = LetterKind::Release;
    let template = checked_template(project, kind)?;
    let values = release_letter_values(project, invoice, date)?;
    let recipient = address_lines(project.info().client(), "");
    if recipient.is_empty() {
        return Err(
            "Für das Anschreiben fehlt der Bauherr bzw. Auftraggeber (Projektstammdaten)."
                .to_string(),
        );
    }
    Ok(PrintLetters {
        title: format!("{} – {}", kind.label(), project.name().as_str()),
        logo: None,
        letters: vec![letter(project, &template, &values, recipient, date, true)],
        frame: Default::default(),
        look: Default::default(),
    })
}

fn checked_template(project: &Project, kind: LetterKind) -> Result<LetterTemplate, String> {
    let template = project.letter_template(kind);
    let unknown = template.unknown_placeholders();
    if !unknown.is_empty() {
        return Err(format!(
            "Unbekannte Platzhalter in der Vorlage: {}.",
            unknown
                .iter()
                .map(|name| format!("{{{name}}}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(template)
}

/// One letter: sender, recipient, reference lines, subject, body, signature.
fn letter(
    project: &Project,
    template: &LetterTemplate,
    values: &[(String, String)],
    recipient: Vec<String>,
    date: &str,
    billing: bool,
) -> PrintLetter {
    let planner = &project.info().planner;
    let sender = address_lines(planner, "");
    let (subject, body) = template.filled(values);
    let value = |key: &str| {
        values
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let mut references = vec![("Datum".to_string(), date.to_string())];
    let keys: &[(&str, &str)] = if billing {
        &[
            ("Projekt", "projekt"),
            ("LV", "lv"),
            ("Auftrags-Nr.", "auftrag.nummer"),
            ("Rechnung", "rechnung.nummer"),
        ]
    } else {
        &[
            ("Projekt", "projekt"),
            ("LV", "lv"),
            ("Vergabenummer", "vergabenummer"),
        ]
    };
    for (label, key) in keys {
        let text = value(key);
        if text != "–" && !text.is_empty() {
            references.push((label.to_string(), text));
        }
    }
    if !planner.phone.trim().is_empty() {
        references.push(("Telefon".to_string(), planner.phone.trim().to_string()));
    }
    if !planner.email.trim().is_empty() {
        references.push(("E-Mail".to_string(), planner.email.trim().to_string()));
    }
    PrintLetter {
        return_line: sender.join(" · "),
        sender,
        recipient,
        references,
        subject,
        body: blocks(project, &body),
        signature: [planner.name1.trim(), planner.name2.trim()]
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    }
}

/// Name, Zusatz, „z. Hd.“, Straße, PLZ Ort.
fn address_lines(address: &Address, contact: &str) -> Vec<String> {
    let city = format!("{} {}", address.postal_code.trim(), address.city.trim());
    let contact = if contact.trim().is_empty() {
        String::new()
    } else {
        format!("z. Hd. {}", contact.trim())
    };
    [
        address.name1.trim(),
        address.name2.trim(),
        contact.as_str(),
        address.street.trim(),
        city.trim(),
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
    use proava_domain::{Address, InvitedBidder, LetterKind, Lv, OzMask, Project, ProjectName};

    use super::letters_to_print;

    #[test]
    fn one_letter_per_recipient_with_address_block() {
        let mut lv = Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.set_number("1");
        let mut project = Project::new(ProjectName::parse("Schule").unwrap(), "2026", lv);
        project.info_mut().planner = Address {
            name1: "Planungsbüro Muster".into(),
            city: "Musterstadt".into(),
            postal_code: "12345".into(),
            phone: "07222 1".into(),
            ..Address::default()
        };
        let mut invited = InvitedBidder::new(Address {
            name1: "Müller GmbH".into(),
            street: "Hafen 2".into(),
            postal_code: "54321".into(),
            city: "Beispielstadt".into(),
            ..Address::default()
        });
        invited.contact_person = "Frau Müller".into();
        let first = project.add_invitee(invited);
        let second = project.add_invitee(InvitedBidder::new(Address::named("Schmidt KG")));
        let letters = letters_to_print(
            &project,
            LetterKind::Invitation,
            &[first, second],
            "17.11.2026",
        )
        .unwrap();
        assert_eq!(letters.letters.len(), 2);
        let letter = &letters.letters[0];
        assert_eq!(
            letter.recipient,
            vec![
                "Müller GmbH",
                "z. Hd. Frau Müller",
                "Hafen 2",
                "54321 Beispielstadt"
            ]
        );
        assert_eq!(
            letter.return_line,
            "Planungsbüro Muster · 12345 Musterstadt"
        );
        assert!(letter.subject.ends_with("LV 1 – GA"));
        assert!(letter
            .references
            .contains(&("Telefon".into(), "07222 1".into())));
        assert!(letters_to_print(&project, LetterKind::Invitation, &[], "17.11.2026").is_err());
        let mut broken = project.letter_template(LetterKind::Invitation);
        broken.subject = "{falsch}".into();
        project.set_letter_template(broken);
        assert!(
            letters_to_print(&project, LetterKind::Invitation, &[first], "x")
                .unwrap_err()
                .contains("{falsch}")
        );
    }
}
