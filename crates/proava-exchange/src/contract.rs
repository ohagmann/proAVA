//! Auftrags-LV: the LV with the prices of the awarded bid, as GAEB X86/D86
//! and as PDF.

use proava_domain::{
    format_decimal_de, Bid, BidId, ComplementKind, Discount, Project, TextComplement,
};
use proava_gaeb::{GaebDocument, Phase, Totals};
use proava_print::{LvPrintMode, PrintLv};

use crate::text::{address_to_gaeb, short_list};
use crate::{lv_to_gaeb, lv_to_print};

/// The project with the bid's unit prices as planner prices and the
/// bidder's Textergänzungen filled into the positions.
fn project_with_bid(project: &Project, bid: &Bid) -> Project {
    let mut copy = project.clone();
    let lv = copy.lv_mut();
    let ids: Vec<_> = lv.positions().map(|position| position.id()).collect();
    for id in ids {
        let index = lv.index_of(id).expect("position exists");
        let Some(position) = lv.row_mut(index).and_then(|row| row.as_position_mut()) else {
            continue;
        };
        // Auftrag = Angebot: the quantity the bid was priced with (a later
        // change in the LV is a Mehr-/Mindermenge for a Nachtrag).
        let offered = bid.offered_quantity(position);
        if offered != position.quantity() {
            position.set_quantity(offered);
        }
        position.set_unit_price(bid.unit_price(id));
        // EP-Anteile belong to the planner's price, not to the bid.
        position.set_price_components(Vec::new());
        let mut complements = position.complements().to_vec();
        for text in bid.complements_of(id) {
            match complements
                .iter_mut()
                .find(|gap| gap.kind == ComplementKind::Bidder && gap.marker.trim() == text.marker)
            {
                Some(gap) => gap.body = text.text.clone(),
                None => complements.push(TextComplement {
                    marker: text.marker.clone(),
                    kind: ComplementKind::Bidder,
                    body: text.text.clone(),
                    ..TextComplement::default()
                }),
            }
        }
        position.set_complements(complements);
    }
    copy
}

fn find_bid(project: &Project, id: BidId) -> Result<&Bid, String> {
    project
        .bids()
        .iter()
        .find(|bid| bid.id() == id)
        .ok_or_else(|| "Das Angebot gibt es nicht (mehr).".to_string())
}

/// Warnings that hold for every form of the Auftrags-LV.
fn contract_warnings(project: &Project, bid: &Bid) -> Vec<String> {
    let mut warnings = Vec::new();
    if !bid.is_awarded() {
        warnings.push(format!(
            "Das Angebot von {} ist (noch) nicht beauftragt.",
            bid.name()
        ));
    }
    let missing: Vec<String> = bid
        .missing_positions(project.lv())
        .iter()
        .map(|position| position.ordinal().to_string())
        .collect();
    if !missing.is_empty() {
        warnings.push(format!(
            "Positionen ohne Preis im Angebot ({}): {}",
            missing.len(),
            short_list(&missing, 15)
        ));
    }
    warnings
}

/// Auftrags-LV as GAEB phase 86: LV of the active LV with the bid's unit
/// prices and Textergänzungen, the bidder as CTR, the client as OWN, award
/// number, execution period and the bid's discount in the totals.
pub fn contract_to_gaeb(
    project: &Project,
    bid_id: BidId,
) -> Result<(GaebDocument, Vec<String>), String> {
    let bid = find_bid(project, bid_id)?;
    let priced = project_with_bid(project, bid);
    let (mut doc, export_warnings) = lv_to_gaeb(&priced, Phase::Dp86);
    let mut warnings = contract_warnings(project, bid);
    let (images, _) = project.lv_images();
    if images > 0 {
        warnings.push(format!(
            "{} aus den Langtexten gehen nicht ins Auftrags-LV (nur ins PDF).",
            proava_domain::wording::count(images, "Bild", "Bilder")
        ));
    }
    warnings.extend(
        export_warnings
            .into_iter()
            .filter(|warning| !warning.starts_with("Positionen ohne Einheitspreis")),
    );
    if project.info().client().name1.trim().is_empty() {
        warnings.push(
            "Kein Auftraggeber in den Projektdaten – GAEB 86 verlangt ihn (leer geschrieben)."
                .to_string(),
        );
    }
    doc.award.bidder = Some(address_to_gaeb(bid.bidder()));
    doc.award.bidder_dp_no = bid.exchange_number().trim().to_string();
    doc.award.bid_date = bid.bid_date().map(str::to_string);

    let lv = project.lv();
    let net = bid.net_total(lv);
    let after = bid.total_after_discount(lv);
    let vat = lv.vat(after);
    let mut totals = Totals {
        total: Some(net.to_decimal()),
        vat_percent: Some(lv.vat_percent()),
        total_net: Some(after.to_decimal()),
        vat_amount: Some(vat.to_decimal()),
        total_gross: Some((after + vat).to_decimal()),
        ..Totals::default()
    };
    match bid.discount() {
        Discount::None => {}
        Discount::Percent(percent) => {
            totals.discount_percent = Some(percent);
            totals.total_after_discount = Some(after.to_decimal());
        }
        Discount::Amount(amount) => {
            totals.discount_amount = Some(amount.to_decimal());
            totals.total_after_discount = Some(after.to_decimal());
        }
    }
    doc.award.boq.totals = Some(totals);
    Ok((doc, warnings))
}

/// Auftrags-LV as PDF model: priced LV with the bid's prices, parties
/// (Auftraggeber, Auftragnehmer, award), the bidder's Textergänzungen and
/// the discount in the totals.
pub fn contract_to_print(
    project: &Project,
    bid_id: BidId,
    date: &str,
) -> Result<(PrintLv, Vec<String>), String> {
    bid_lv_to_print(project, bid_id, date, true)
}

/// LV with the prices of one bid (Bieterpreise) as PDF model: like the
/// Auftrags-LV, but headed as the bid and without the award.
pub fn bid_prices_to_print(
    project: &Project,
    bid_id: BidId,
    date: &str,
) -> Result<(PrintLv, Vec<String>), String> {
    bid_lv_to_print(project, bid_id, date, false)
}

fn bid_lv_to_print(
    project: &Project,
    bid_id: BidId,
    date: &str,
    contract: bool,
) -> Result<(PrintLv, Vec<String>), String> {
    let bid = find_bid(project, bid_id)?;
    let priced = project_with_bid(project, bid);
    let mut print = lv_to_print(&priced, LvPrintMode::Priced, date);
    print.header.document_kind = Some(if contract {
        "Auftrags-LV".to_string()
    } else {
        format!("Angebot {}", bid.name())
    });
    let info = project.info();
    let mut parties = Vec::new();
    if !info.client().one_line().is_empty() {
        parties.push(("Auftraggeber".to_string(), info.client().one_line()));
    }
    parties.push((
        if contract { "Auftragnehmer" } else { "Bieter" }.to_string(),
        bid.bidder().one_line(),
    ));
    if !info.award_number.trim().is_empty() {
        parties.push(("Vergabenummer".to_string(), info.award_number.clone()));
    }
    if let Some(date) = bid.bid_date() {
        parties.push((
            "Angebot vom".to_string(),
            crate::text::format_deadline(Some(date), None),
        ));
    }
    if let Some(award) = bid.award().filter(|_| contract) {
        parties.push((
            "Auftrag erteilt".to_string(),
            format!("{} – Auftragssumme netto {}", award.date, award.total),
        ));
    }
    let period = [info.execution_start.trim(), info.execution_end.trim()];
    if contract && period.iter().any(|part| !part.is_empty()) {
        parties.push(("Ausführungszeit".to_string(), period.join(" bis ")));
    }
    print.parties = parties;

    let lv = project.lv();
    let net = bid.net_total(lv);
    print.net_total = Some(net.to_decimal());
    let discount = bid.discount_amount(net);
    if !discount.is_zero() {
        let label = match bid.discount() {
            Discount::Percent(percent) => format!(
                "Nachlass {} %",
                format_decimal_de(percent, percent.normalize().scale())
            ),
            _ => "Nachlass".to_string(),
        };
        print.discount = Some((label, discount.to_decimal()));
    }
    let mut warnings = contract_warnings(project, bid);
    if !contract {
        warnings.retain(|warning| !warning.contains("nicht beauftragt"));
    }
    Ok((print, warnings))
}
