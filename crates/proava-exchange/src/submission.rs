//! Bieterverzeichnis and bids → Niederschrift über die Angebotsöffnung.

use proava_domain::{Bid, Discount, InviteStatus, Project};
use proava_print::{PrintHeader, PrintSubmission, SubmissionBidder};

/// The Niederschrift for the active LV: every invited bidder in list order
/// (with the linked bid's sums), then bids nobody invited.
pub fn submission_to_print(project: &Project, date: &str) -> PrintSubmission {
    let info = project.info();
    let lv = project.lv();
    let mut facts = Vec::new();
    let mut fact = |label: &str, value: &str| {
        if !value.trim().is_empty() {
            facts.push((label.to_string(), value.trim().to_string()));
        }
    };
    fact("Vergabeart", &info.award_procedure);
    fact("Vergabenummer", &info.award_number);
    fact("Eröffnungstermin", &info.submission_deadline);
    fact("Ort", &info.submission_place);
    fact("Bindefrist", &info.binding_period_end);
    let invited = project.invitees().len();
    let received = project.bids().len();
    fact(
        "Bieter",
        &format!(
            "{invited} angefragt, {received} Angebot{} eingegangen",
            if received == 1 { "" } else { "e" }
        ),
    );

    let line = |bid: Option<&Bid>| -> (
        Option<proava_domain::Decimal>,
        Option<(String, proava_domain::Decimal)>,
    ) {
        let Some(bid) = bid else {
            return (None, None);
        };
        let net = bid.net_total(lv);
        let amount = bid.discount_amount(net).to_decimal().abs();
        let discount = match bid.discount() {
            Discount::None => None,
            Discount::Percent(percent) => Some((format!("{} %", german(percent)), amount)),
            Discount::Amount(_) => Some(("Betrag".to_string(), amount)),
        };
        (Some(net.to_decimal()), discount)
    };

    let mut bidders = Vec::new();
    for invitee in project.invitees() {
        let bid = invitee
            .bid
            .and_then(|id| project.bids().iter().find(|bid| bid.id() == id));
        let (net, discount) = line(bid);
        let status = if bid.is_some_and(Bid::is_excluded) {
            InviteStatus::Excluded
        } else {
            invitee.status
        };
        bidders.push(SubmissionBidder {
            name: invitee.name().to_string(),
            place: invitee.address.city.trim().to_string(),
            received: invitee.received.clone(),
            status: status.label().to_string(),
            net,
            discount,
            side_offers: invitee.side_offers,
            remark: invitee.remark.clone(),
        });
    }
    for bid in project.bids() {
        if project.invitee_of_bid(bid.id()).is_some() {
            continue;
        }
        let (net, discount) = line(Some(bid));
        bidders.push(SubmissionBidder {
            name: bid.name().to_string(),
            place: bid.bidder().city.trim().to_string(),
            received: bid.bid_date().map(str::to_string).unwrap_or_default(),
            status: if bid.is_excluded() {
                InviteStatus::Excluded.label().to_string()
            } else {
                InviteStatus::Submitted.label().to_string()
            },
            net,
            discount,
            side_offers: false,
            remark: "nicht im Bieterverzeichnis".to_string(),
        });
    }

    let lv_name = match (lv.number().trim(), lv.title().trim()) {
        ("", title) => title.to_string(),
        (number, title) => format!("LV {number} – {title}"),
    };
    PrintSubmission {
        header: PrintHeader {
            project_name: project.name().as_str().to_string(),
            project_title: info.label.clone(),
            client: info.client().one_line(),
            lv_name,
            date: date.to_string(),
            author: info.planner.name1.clone(),
            document_kind: None,
            logo: None,
            frame: Default::default(),
            look: Default::default(),
            currency: proava_domain::price_check::currency_symbol(lv.currency()),
        },
        facts,
        bidders,
        vat_percent: lv.vat_percent(),
        notes: vec![
            "Die Angebote lagen bei der Öffnung verschlossen vor. Die Angebotssummen wurden \
             aus den Einheitspreisen errechnet; die Prüfung der Angebote bleibt vorbehalten."
                .to_string(),
        ],
        signatures: vec![
            "Verhandlungsleiter/in".to_string(),
            "Schriftführer/in".to_string(),
        ],
        header_text: String::new(),
        footer_text: String::new(),
    }
}

fn german(value: proava_domain::Decimal) -> String {
    proava_domain::format_decimal_de(value, 3)
        .trim_end_matches('0')
        .trim_end_matches(',')
        .to_string()
}

#[cfg(test)]
mod tests {
    use proava_domain::{
        Address, Bid, BidPrice, Discount, InviteStatus, InvitedBidder, Lv, OzMask, Position,
        Project, ProjectName,
    };

    use super::submission_to_print;

    #[test]
    fn invitees_with_their_bids_then_uninvited_bids() {
        let mut lv = Lv::new("GA", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap());
        lv.set_number("1");
        let id = lv.push_position(Position::new(
            "0010",
            "Fühler",
            "10".parse().unwrap(),
            "St",
            None,
        ));
        let mut project = Project::new(ProjectName::parse("Schule").unwrap(), "2026", lv);
        project.info_mut().submission_deadline = "15.12.2026, 10:00 Uhr".into();
        let price = |value: &str| BidPrice {
            unit_price: Some(value.parse().unwrap()),
            stated_total: None,
            not_offered: false,
        };
        let mut first = Bid::new(Address::named("Müller GmbH"), "m.x84");
        first.set_price(id, price("100"));
        first.set_discount(Discount::Percent("3".parse().unwrap()));
        let first = project.add_bid(first);
        let mut stranger = Bid::new(Address::named("Fremd KG"), "f.x84");
        stranger.set_price(id, price("90"));
        project.add_bid(stranger);
        let mueller = project.add_invitee(InvitedBidder::new(Address::named("Müller GmbH")));
        let mut refused = InvitedBidder::new(Address::named("Absage AG"));
        refused.status = InviteStatus::NoBid;
        project.add_invitee(refused);
        project.link_bid(first, Some(mueller)).unwrap();

        let print = submission_to_print(&project, "15.12.2026");
        assert_eq!(print.bidders.len(), 3);
        assert_eq!(print.bidders[0].net, Some("1000".parse().unwrap()));
        assert_eq!(
            print.bidders[0].discount,
            Some(("3 %".into(), "30".parse().unwrap()))
        );
        assert_eq!(print.bidders[1].status, "kein Angebot");
        assert_eq!(print.bidders[1].net, None);
        assert_eq!(print.bidders[2].remark, "nicht im Bieterverzeichnis");
        assert!(print
            .facts
            .contains(&("Eröffnungstermin".into(), "15.12.2026, 10:00 Uhr".into())));
        assert!(print.facts.contains(&(
            "Bieter".into(),
            "2 angefragt, 2 Angebote eingegangen".into()
        )));
        assert_eq!(print.header.lv_name, "LV 1 – GA");
    }
}
