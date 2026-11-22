//! Schriftverkehr: letters to the bidders of an LV from templates with
//! placeholders (Aufforderung zur Angebotsabgabe, Zuschlag, Absage) and the
//! letter to the client with an invoice release (Abrechnung).

use crate::bid::Address;
use crate::invitation::{InviteStatus, InvitedBidder, InviteeId};
use crate::project::Project;
use crate::rich_text::RichText;

/// Kinds of letters; each has its own template in the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LetterKind {
    /// Aufforderung zur Abgabe eines Angebots.
    Invitation,
    /// Zuschlagsschreiben an den beauftragten Bieter.
    Award,
    /// Information der nicht berücksichtigten Bieter (§ 134 GWB).
    Rejection,
    /// Abrechnung: Rechnungsfreigabe an den Auftraggeber/Bauherrn (one
    /// letter about one invoice, not to the bidders).
    Release,
}

impl LetterKind {
    pub const ALL: [LetterKind; 4] = [
        LetterKind::Invitation,
        LetterKind::Award,
        LetterKind::Rejection,
        LetterKind::Release,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LetterKind::Invitation => "Aufforderung zur Angebotsabgabe",
            LetterKind::Award => "Zuschlagsschreiben",
            LetterKind::Rejection => "Absage / Information nach § 134 GWB",
            LetterKind::Release => "Freigabe an den Bauherrn",
        }
    }

    /// Key in the project file and the MCP server.
    pub fn key(self) -> &'static str {
        match self {
            LetterKind::Invitation => "aufforderung",
            LetterKind::Award => "zuschlag",
            LetterKind::Rejection => "absage",
            LetterKind::Release => "freigabe",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let key = key.trim().to_lowercase();
        Self::ALL.into_iter().find(|kind| kind.key() == key)
    }

    /// Part of file names.
    pub fn file_label(self) -> &'static str {
        match self {
            LetterKind::Invitation => "Aufforderung",
            LetterKind::Award => "Zuschlag",
            LetterKind::Rejection => "Absage",
            LetterKind::Release => "Freigabe",
        }
    }

    /// The letter goes to the client (Auftraggeber/Bauherr) about one
    /// invoice of the Abrechnung, not to invited bidders.
    pub fn to_client(self) -> bool {
        self == LetterKind::Release
    }
}

/// A letter template: subject line and body with `{platzhalter}`.
#[derive(Debug, Clone, PartialEq)]
pub struct LetterTemplate {
    pub kind: LetterKind,
    pub subject: String,
    pub body: RichText,
}

/// Placeholders with a description, for the editor and the documentation.
pub const PLACEHOLDERS: [(&str, &str); 44] = [
    ("bieter.name", "Firma des Bieters"),
    ("bieter.zusatz", "Namenszusatz des Bieters"),
    ("bieter.strasse", "Straße des Bieters"),
    ("bieter.plz_ort", "PLZ und Ort des Bieters"),
    ("bieter.anschrift", "Anschrift in einer Zeile"),
    ("bieter.email", "E-Mail des Bieters"),
    ("ansprechpartner", "Ansprechpartner beim Bieter"),
    ("anrede", "Anrede, z. B. „Sehr geehrte Damen und Herren,“"),
    ("projekt", "Projektname"),
    ("projektbezeichnung", "Projektbezeichnung"),
    ("baumassnahme", "Baumaßnahme"),
    ("bauherr", "Bauherr / Auftraggeber (eine Zeile)"),
    ("baustelle", "Baustelle (eine Zeile)"),
    ("lv", "LV mit Nummer und Bezeichnung"),
    ("lv.nummer", "Nummer des LV"),
    ("vergabeart", "Vergabeart"),
    ("vergabenummer", "Vergabenummer"),
    ("abgabetermin", "Abgabetermin"),
    ("abgabeort", "Abgabeort"),
    ("bindefrist", "Zuschlags- und Bindefrist"),
    ("ausfuehrung_beginn", "Ausführungsbeginn"),
    ("ausfuehrung_ende", "Ausführungsende"),
    ("planer.name", "Aufsteller / Planer"),
    ("planer.anschrift", "Anschrift des Planers (eine Zeile)"),
    ("planer.telefon", "Telefon des Planers"),
    ("planer.email", "E-Mail des Planers"),
    ("datum", "Datum des Schreibens"),
    (
        "angebot.summe",
        "Angebotssumme netto nach Nachlass des Bieters",
    ),
    ("auftrag.summe", "Auftragssumme des beauftragten Angebots"),
    ("zuschlag.bieter", "Bieter, der den Zuschlag erhalten soll"),
    (
        "absagegrund",
        "Grund der Nichtberücksichtigung (Bieterverzeichnis)",
    ),
    (
        "auftragnehmer",
        "Auftragnehmer (Firma des festgeschriebenen Auftrags)",
    ),
    ("auftrag.nummer", "Auftrags-Nr. des Auftraggebers"),
    ("rechnung.nummer", "Rechnungsnummer der Firma (Abrechnung)"),
    (
        "rechnung.art",
        "Art der Rechnung, z. B. „3. Abschlagsrechnung“",
    ),
    ("rechnung.datum", "Rechnungsdatum"),
    ("rechnung.eingang", "Eingang der Rechnung"),
    ("rechnung.zahlungsziel", "Zahlungsziel der Rechnung"),
    ("rechnung.forderung", "Forderung der Firma netto"),
    (
        "rechnung.ust",
        "Umsatzsteuer der Rechnung, z. B. „USt 19 %“",
    ),
    ("freigabe.netto", "Freigabebetrag netto"),
    ("freigabe.brutto", "Freigabebetrag brutto"),
    (
        "freigaben.gesamt",
        "Freigaben gesamt einschließlich dieser, netto",
    ),
    ("einbehalt", "Einbehalt nach dieser Freigabe, netto"),
];

/// Placeholders of the Abrechnung: filled for one invoice
/// ([`release_letter_values`]), in the other letters „–“.
const BILLING_PLACEHOLDERS: [&str; 11] = [
    "rechnung.nummer",
    "rechnung.art",
    "rechnung.datum",
    "rechnung.eingang",
    "rechnung.zahlungsziel",
    "rechnung.forderung",
    "rechnung.ust",
    "freigabe.netto",
    "freigabe.brutto",
    "freigaben.gesamt",
    "einbehalt",
];

const INVITATION: &str = "Sehr geehrte Damen und Herren,

für die Baumaßnahme „{projektbezeichnung}“ ({projekt}) fordern wir Sie im Auftrag des Bauherrn {bauherr} auf, ein Angebot für die Leistungen des Leistungsverzeichnisses {lv} abzugeben (Vergabeart: {vergabeart}, Vergabenummer: {vergabenummer}).

Diesem Schreiben liegen bei:
- Leistungsverzeichnis als GAEB-Datei (X83) und als PDF
- Projektangaben und Vertragsbedingungen (Deckblatt des Leistungsverzeichnisses)
- Pläne und weitere Unterlagen nach beiliegender Liste

Bitte reichen Sie Ihr Angebot bis {abgabetermin} ein: {abgabeort}. Später eingehende Angebote können nicht berücksichtigt werden. Tragen Sie die Preise bitte in die GAEB-Datei (X84) ein und legen Sie das unterschriebene Angebotsschreiben bei. Änderungen an den Vergabeunterlagen sind nicht zulässig.

Zuschlags- und Bindefrist: {bindefrist}. Vorgesehene Ausführungszeit: {ausfuehrung_beginn} bis {ausfuehrung_ende}.

Für Rückfragen steht Ihnen {planer.name} zur Verfügung (Telefon {planer.telefon}, E-Mail {planer.email}).

Mit freundlichen Grüßen";

const AWARD: &str = "Sehr geehrte Damen und Herren,

im Auftrag des Bauherrn {bauherr} erteilen wir Ihnen auf Ihr Angebot für die Leistungen {lv} der Baumaßnahme „{projektbezeichnung}“ (Vergabenummer {vergabenummer}) den Zuschlag.

Die Auftragssumme beträgt {auftrag.summe} netto zuzüglich der gesetzlichen Umsatzsteuer. Grundlage des Vertrags sind die Vergabeunterlagen und Ihr Angebot. Vorgesehene Ausführungszeit: {ausfuehrung_beginn} bis {ausfuehrung_ende}.

Bitte bestätigen Sie den Eingang dieses Schreibens.

Mit freundlichen Grüßen";

const REJECTION: &str = "Sehr geehrte Damen und Herren,

vielen Dank für Ihr Angebot zu den Leistungen {lv} der Baumaßnahme „{projektbezeichnung}“ (Vergabenummer {vergabenummer}).

Wir teilen Ihnen mit, dass Ihr Angebot nicht berücksichtigt werden kann. Der Zuschlag soll auf das Angebot von {zuschlag.bieter} erteilt werden. Der Vertrag wird nicht vor Ablauf der Frist nach § 134 Abs. 2 GWB geschlossen.

Grund der Nichtberücksichtigung: {absagegrund}

Mit freundlichen Grüßen";

const RELEASE: &str = "Sehr geehrte Damen und Herren,

zur Baumaßnahme „{projektbezeichnung}“ ({projekt}) erhalten Sie die geprüfte {rechnung.art} Nr. {rechnung.nummer} vom {rechnung.datum} der Firma {auftragnehmer} für die Leistungen {lv} (Auftrags-Nr. {auftrag.nummer}).

Die Rechnung ist am {rechnung.eingang} eingegangen; gefordert sind {rechnung.forderung} netto. Nach sachlicher und rechnerischer Prüfung geben wir zur Zahlung frei:

{freigabe.brutto} brutto ({freigabe.netto} netto, {rechnung.ust})

Zahlungsziel laut Rechnung: {rechnung.zahlungsziel}. Den Rechenweg mit Abzügen, Sicherheitseinbehalt und den bisherigen Freigaben zeigt die beiliegende Rechnungsfreigabe, die Prüfung je Position die Aufmaßprüfung. Freigegeben sind damit insgesamt {freigaben.gesamt} netto; einbehalten bleiben {einbehalt} netto.

Bitte prüfen Sie vor der Zahlung, ob eine gültige Freistellungsbescheinigung des Auftragnehmers nach § 48b EStG vorliegt; sonst sind 15 % des Auszahlungsbetrags als Bauabzugsteuer an das Finanzamt abzuführen.

Anlagen:
- Rechnungsfreigabe
- Aufmaßprüfung
- Rechnung des Auftragnehmers

Mit freundlichen Grüßen";

impl LetterTemplate {
    /// The template proAVA brings along.
    pub fn standard(kind: LetterKind) -> Self {
        let (subject, body) = match kind {
            LetterKind::Invitation => ("Aufforderung zur Abgabe eines Angebots – {lv}", INVITATION),
            LetterKind::Award => ("Zuschlag – {lv}", AWARD),
            LetterKind::Rejection => (
                "Information über den Ausgang des Vergabeverfahrens – {lv}",
                REJECTION,
            ),
            LetterKind::Release => (
                "Rechnungsfreigabe {rechnung.art} {rechnung.nummer} – {auftragnehmer} – {lv}",
                RELEASE,
            ),
        };
        Self {
            kind,
            subject: subject.to_string(),
            body: crate::rich_text::RichText::plain(body),
        }
    }

    /// Placeholders in subject or body that proAVA does not know.
    pub fn unknown_placeholders(&self) -> Vec<String> {
        let mut unknown = Vec::new();
        for text in [self.subject.clone(), self.body.to_plain()] {
            for name in placeholders_in(&text) {
                if !PLACEHOLDERS.iter().any(|(key, _)| *key == name) && !unknown.contains(&name) {
                    unknown.push(name);
                }
            }
        }
        unknown
    }

    /// Subject and body with the values of one recipient.
    pub fn filled(&self, values: &[(String, String)]) -> (String, RichText) {
        let mut subject = self.subject.clone();
        let mut body = self.body.clone();
        for (key, value) in values {
            let marker = format!("{{{key}}}");
            subject = subject.replace(&marker, value);
            body.replace_all(&marker, value, true);
        }
        (subject, body)
    }
}

/// `{name}` markers in a text (letters, digits, `.`, `_`).
pub fn placeholders_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '.' || c == '_')
                {
                    found.push(name.to_string());
                }
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    found
}

fn or_dash(text: &str) -> String {
    if text.trim().is_empty() {
        "–".to_string()
    } else {
        text.trim().to_string()
    }
}

/// „PLZ Ort“
fn postal_city(address: &Address) -> String {
    format!("{} {}", address.postal_code.trim(), address.city.trim())
        .trim()
        .to_string()
}

/// Values of all placeholders for one invited bidder of the active LV;
/// empty facts become „–“ so the letter shows what is missing.
pub fn letter_values(
    project: &Project,
    invitee: &InvitedBidder,
    date: &str,
) -> Vec<(String, String)> {
    let info = project.info();
    let lv = project.lv();
    let lv_label = match (lv.number().trim(), lv.title().trim()) {
        ("", title) => title.to_string(),
        (number, title) => format!("LV {number} – {title}"),
    };
    let bid = invitee
        .bid
        .and_then(|id| project.bids().iter().find(|bid| bid.id() == id));
    let awarded = project.awarded_bid();
    // In the Währung of the LV („1.234,56 €“, „… CHF“).
    let money = |value: crate::money::Money| crate::price_check::amount(value, lv.currency());
    let values: Vec<(&str, String)> = vec![
        ("bieter.name", or_dash(&invitee.address.name1)),
        ("bieter.zusatz", invitee.address.name2.trim().to_string()),
        ("bieter.strasse", invitee.address.street.trim().to_string()),
        ("bieter.plz_ort", postal_city(&invitee.address)),
        ("bieter.anschrift", or_dash(&invitee.address.one_line())),
        ("bieter.email", or_dash(&invitee.address.email)),
        ("ansprechpartner", or_dash(&invitee.contact_person)),
        ("anrede", "Sehr geehrte Damen und Herren,".to_string()),
        ("projekt", project.name().as_str().to_string()),
        ("projektbezeichnung", or_dash(&info.label)),
        (
            "baumassnahme",
            or_dash(&info.description.replace('\n', ", ")),
        ),
        ("bauherr", or_dash(&info.owner.one_line())),
        ("baustelle", or_dash(&info.site.one_line())),
        ("lv", lv_label),
        ("lv.nummer", or_dash(lv.number())),
        ("vergabeart", or_dash(&info.award_procedure)),
        ("vergabenummer", or_dash(&info.award_number)),
        ("abgabetermin", or_dash(&info.submission_deadline)),
        ("abgabeort", or_dash(&info.submission_place)),
        ("bindefrist", or_dash(&info.binding_period_end)),
        ("ausfuehrung_beginn", or_dash(&info.execution_start)),
        ("ausfuehrung_ende", or_dash(&info.execution_end)),
        ("planer.name", or_dash(&info.planner.name1)),
        ("planer.anschrift", or_dash(&info.planner.one_line())),
        ("planer.telefon", or_dash(&info.planner.phone)),
        ("planer.email", or_dash(&info.planner.email)),
        ("datum", date.to_string()),
        (
            "angebot.summe",
            bid.map(|bid| money(bid.total_after_discount(lv)))
                .unwrap_or_else(|| "–".into()),
        ),
        (
            "auftrag.summe",
            awarded
                .and_then(|bid| bid.award().map(|award| award.total))
                .map(money)
                .unwrap_or_else(|| "–".into()),
        ),
        (
            "zuschlag.bieter",
            awarded
                .map(|bid| bid.name().to_string())
                .unwrap_or_else(|| "–".into()),
        ),
        (
            "absagegrund",
            or_dash(
                &invitee
                    .rejection_reason
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        ),
        (
            "auftragnehmer",
            or_dash(
                project
                    .contract()
                    .map(|contract| contract.contractor.as_str())
                    .unwrap_or_default(),
            ),
        ),
        (
            "auftrag.nummer",
            or_dash(
                project
                    .contract()
                    .map(|contract| contract.number.as_str())
                    .unwrap_or_default(),
            ),
        ),
    ];
    let mut values: Vec<(String, String)> = values
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect();
    values.extend(
        BILLING_PLACEHOLDERS
            .iter()
            .map(|key| (key.to_string(), "–".to_string())),
    );
    values
}

/// Values of all placeholders for the letter to the client about `invoice`
/// (None = the last one) of the active LV: the general ones, the contractor
/// as „bieter“ and the invoice with its release.
pub fn release_letter_values(
    project: &Project,
    invoice: Option<crate::InvoiceId>,
    date: &str,
) -> Result<Vec<(String, String)>, String> {
    let contract = project
        .contract()
        .ok_or("Für das LV ist noch kein Auftrag festgeschrieben (Reiter Abrechnung).")?;
    let context = project.billing_context().ok_or("Kein Auftrag.")?;
    let id = invoice
        .or_else(|| {
            project
                .billing()
                .invoices()
                .last()
                .map(|invoice| invoice.id)
        })
        .ok_or("Im LV ist noch keine Rechnung erfasst.")?;
    let check = project.billing().check(&context, id)?;
    let invoice = project
        .billing()
        .invoice(id)
        .ok_or("Die Rechnung gibt es nicht (mehr).")?;
    // The contractor as „bieter“: from the Bieterverzeichnis when the bid is
    // linked there, else from the bid or the name of the contract.
    let contractor = contract
        .bid
        .and_then(|bid| {
            project
                .invitees()
                .iter()
                .find(|invitee| invitee.bid == Some(bid))
                .cloned()
        })
        .unwrap_or_else(|| {
            let address = contract
                .bid
                .and_then(|bid| {
                    project
                        .bids()
                        .iter()
                        .find(|candidate| candidate.id() == bid)
                })
                .map(|bid| bid.bidder().clone())
                .unwrap_or_else(|| Address::named(contract.contractor.clone()));
            InvitedBidder::new(address)
        });
    let mut values = letter_values(project, &contractor, date);
    let release = &check.release;
    let claimed = invoice.claimed_net.unwrap_or(check.claimed_total);
    let currency = project.lv().currency().to_string();
    let money = |value: crate::money::Money| crate::price_check::amount(value, &currency);
    let billing: Vec<(&str, String)> = vec![
        ("rechnung.nummer", or_dash(&invoice.number)),
        ("rechnung.art", check.label.clone()),
        ("rechnung.datum", or_dash(&invoice.date)),
        ("rechnung.eingang", or_dash(&invoice.received)),
        ("rechnung.zahlungsziel", or_dash(&invoice.due)),
        ("rechnung.forderung", money(claimed)),
        ("rechnung.ust", invoice.vat.label()),
        ("freigabe.netto", money(release.release_net)),
        ("freigabe.brutto", money(release.release_gross)),
        ("freigaben.gesamt", money(release.released_total)),
        ("einbehalt", money(release.withheld)),
    ];
    for (key, value) in billing {
        if let Some(entry) = values.iter_mut().find(|(known, _)| known == key) {
            entry.1 = value;
        }
    }
    Ok(values)
}

/// Who gets a letter of this kind by default: everyone on the list
/// (Aufforderung), the bidder of the awarded bid (Zuschlag), bidders with a
/// bid that was not awarded or who were excluded (Absage).
pub fn default_recipients(project: &Project, kind: LetterKind) -> Vec<InviteeId> {
    let awarded = project.awarded_bid().map(|bid| bid.id());
    project
        .invitees()
        .iter()
        .filter(|invitee| match kind {
            LetterKind::Invitation => true,
            LetterKind::Award => awarded.is_some() && invitee.bid == awarded,
            LetterKind::Rejection => {
                (invitee.bid.is_some() && invitee.bid != awarded)
                    || (invitee.status == InviteStatus::Excluded && invitee.bid != awarded)
            }
            // Goes to the client, not to the Bieterverzeichnis.
            LetterKind::Release => false,
        })
        .map(|invitee| invitee.id)
        .collect()
}

impl Project {
    /// The template of this kind: the project's own or the standard one.
    pub fn letter_template(&self, kind: LetterKind) -> LetterTemplate {
        self.letter_templates()
            .iter()
            .find(|template| template.kind == kind)
            .cloned()
            .unwrap_or_else(|| LetterTemplate::standard(kind))
    }

    /// Puts back a whole set of stored templates (Rückgängig) and returns the
    /// set that was there.
    pub fn replace_letter_templates(
        &mut self,
        templates: Vec<LetterTemplate>,
    ) -> Vec<LetterTemplate> {
        std::mem::replace(self.letter_templates_mut(), templates)
    }

    /// Stores a template; the standard text is not stored (keeps files small).
    pub fn set_letter_template(&mut self, template: LetterTemplate) {
        let templates = self.letter_templates_mut();
        templates.retain(|known| known.kind != template.kind);
        if template != LetterTemplate::standard(template.kind) {
            templates.push(template);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{default_recipients, letter_values, placeholders_in, LetterKind, LetterTemplate};
    use crate::{Address, Bid, InvitedBidder, Lv, OzMask, Project, ProjectName};

    fn project() -> Project {
        let mut lv = Lv::new(
            "Gebäudeautomation",
            OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap(),
        );
        lv.set_number("1");
        let mut project = Project::new(ProjectName::parse("Schule").unwrap(), "2026", lv);
        project.info_mut().submission_deadline = "15.12.2026, 10:00 Uhr".into();
        project
    }

    #[test]
    fn standard_templates_know_all_their_placeholders() {
        for kind in LetterKind::ALL {
            let template = LetterTemplate::standard(kind);
            assert!(
                template.unknown_placeholders().is_empty(),
                "{kind:?}: {:?}",
                template.unknown_placeholders()
            );
            assert_eq!(LetterKind::from_key(kind.key()), Some(kind));
        }
        assert_eq!(
            placeholders_in("a {x.y} {} {kaputt b}"),
            vec!["x.y".to_string()]
        );
        let mut odd = LetterTemplate::standard(LetterKind::Invitation);
        odd.subject.push_str(" {gibtsnicht}");
        assert_eq!(odd.unknown_placeholders(), vec!["gibtsnicht".to_string()]);
    }

    #[test]
    fn letters_are_filled_per_bidder() {
        let mut project = project();
        let mut invited = InvitedBidder::new(Address {
            name1: "Müller GmbH".into(),
            city: "Musterstadt".into(),
            postal_code: "12345".into(),
            ..Address::default()
        });
        invited.contact_person = "Frau Müller".into();
        let id = project.add_invitee(invited);
        let invitee = project.invitees()[0].clone();
        let values = letter_values(&project, &invitee, "17.11.2026");
        let (subject, body) = LetterTemplate::standard(LetterKind::Invitation).filled(&values);
        assert_eq!(
            subject,
            "Aufforderung zur Abgabe eines Angebots – LV 1 – Gebäudeautomation"
        );
        let text = body.to_plain();
        assert!(text.contains("bis 15.12.2026, 10:00 Uhr ein"), "{text}");
        assert!(
            text.contains("Vergabenummer: –"),
            "leere Angaben als –: {text}"
        );
        assert!(!text.contains('{'), "{text}");
        assert_eq!(
            default_recipients(&project, LetterKind::Invitation),
            vec![id]
        );
        assert!(default_recipients(&project, LetterKind::Award).is_empty());

        let bid = project.add_bid(Bid::new(Address::named("Müller GmbH"), "m.x84"));
        project.link_bid(bid, Some(id)).unwrap();
        let other = project.add_invitee(InvitedBidder::new(Address::named("Schmidt")));
        let other_bid = project.add_bid(Bid::new(Address::named("Schmidt"), "s.x84"));
        project.link_bid(other_bid, Some(other)).unwrap();
        project.award_bid(bid, "20.01.2027").unwrap();
        assert_eq!(default_recipients(&project, LetterKind::Award), vec![id]);
        assert_eq!(
            default_recipients(&project, LetterKind::Rejection),
            vec![other]
        );
        let values = letter_values(&project, &project.invitees()[1].clone(), "17.11.2026");
        let (_, body) = LetterTemplate::standard(LetterKind::Rejection).filled(&values);
        assert!(body.to_plain().contains("Angebot von Müller GmbH erteilt"));
        assert!(
            body.to_plain().contains("Nichtberücksichtigung: –"),
            "ohne Grund: –"
        );
        project.invitee_mut(other).unwrap().rejection_reason =
            "Angebot nicht das wirtschaftlichste\n(Rang 2)".into();
        let values = letter_values(&project, &project.invitees()[1].clone(), "17.11.2026");
        let (_, body) = LetterTemplate::standard(LetterKind::Rejection).filled(&values);
        assert!(
            body.to_plain()
                .contains("Nichtberücksichtigung: Angebot nicht das wirtschaftlichste (Rang 2)"),
            "{}",
            body.to_plain()
        );
    }

    #[test]
    fn only_changed_templates_are_stored() {
        let mut project = project();
        let mut template = project.letter_template(LetterKind::Award);
        project.set_letter_template(template.clone());
        assert!(project.letter_templates().is_empty());
        template.subject = "Auftrag – {lv}".into();
        project.set_letter_template(template.clone());
        assert_eq!(project.letter_template(LetterKind::Award), template);
        assert_eq!(project.letter_templates().len(), 1);
    }
}
