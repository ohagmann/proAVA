//! Niederschrift über die Angebotsöffnung (Submissionsprotokoll): model,
//! JSON for the template and rendering. Kept apart from the LV printouts.

use rust_decimal::Decimal;
use serde_json::{json, Value};

use crate::format::{breakable, money_in, round_money};
use crate::model::PrintHeader;
use crate::view::{self, Files};
use crate::PrintError;

const SUBMISSION_TEMPLATE: &str = include_str!("../templates/submission.typ");

/// Everything printed in the Niederschrift.
#[derive(Debug, Clone, Default)]
pub struct PrintSubmission {
    pub header: PrintHeader,
    /// Facts about the opening: („Vergabeart“, …), („Eröffnungstermin“, …) …
    pub facts: Vec<(String, String)>,
    pub bidders: Vec<SubmissionBidder>,
    pub vat_percent: Decimal,
    /// Free lines under the table (e.g. Hinweise zu verspäteten Angeboten).
    pub notes: Vec<String>,
    /// Captions under the signature lines, e.g. „Verhandlungsleiter“.
    pub signatures: Vec<String>,
    /// Free text in the page header / footer of every page (print dialog).
    pub header_text: String,
    pub footer_text: String,
}

/// One line of the Niederschrift: a bidder asked or a bid read in.
#[derive(Debug, Clone, Default)]
pub struct SubmissionBidder {
    pub name: String,
    /// Ort des Bieters.
    pub place: String,
    /// Eingang, e.g. „14.12.2026, 09:30 Uhr“.
    pub received: String,
    /// „Angebot abgegeben“, „kein Angebot“, „ausgeschlossen“ …
    pub status: String,
    /// Net sum before discount; None = no bid.
    pub net: Option<Decimal>,
    /// Discount as text (e.g. „3 %“) and amount (positive).
    pub discount: Option<(String, Decimal)>,
    pub side_offers: bool,
    pub remark: String,
}

pub fn render_submission_pdf(submission: &PrintSubmission) -> Result<Vec<u8>, PrintError> {
    render_submission_pdf_counted(submission).map(|(bytes, _)| bytes)
}

/// The Niederschrift together with its page count.
pub fn render_submission_pdf_counted(
    submission: &PrintSubmission,
) -> Result<(Vec<u8>, usize), PrintError> {
    let world = submission_world(submission)?;
    let document = world.layout()?;
    Ok((world.pdf(&document)?, document.pages().len()))
}

pub(crate) fn submission_world(
    submission: &PrintSubmission,
) -> Result<crate::world::PrintWorld, PrintError> {
    let mut files = Files::default();
    let data = data(submission, &mut files);
    crate::build_world(SUBMISSION_TEMPLATE, files, &data)
}

fn data(submission: &PrintSubmission, files: &mut Files) -> Value {
    let vat = submission.vat_percent;
    // In the Währung of the LV.
    let cur = submission.header.currency_sign();
    let money_eur = |value: Decimal| money_in(value, cur);
    let rows: Vec<Value> = submission
        .bidders
        .iter()
        .enumerate()
        .map(|(index, bidder)| {
            let discount = bidder
                .discount
                .as_ref()
                .map(|(_, amount)| *amount)
                .unwrap_or_default();
            let after = bidder.net.map(|net| net - discount);
            let gross = after.map(|after| after + round_money(after * vat / Decimal::ONE_HUNDRED));
            json!({
                // PDF-Lesezeichen under „Angebote“.
                "bm": view::bookmark(2, &[&bidder.name]),
                "nr": (index + 1).to_string(),
                "name": breakable(&bidder.name),
                "place": breakable(&bidder.place),
                "received": bidder.received,
                "status": bidder.status,
                "net": bidder.net.map(money_eur),
                "discount": bidder.discount.as_ref().map(|(label, amount)| {
                    format!("{label}\n−{}", money_eur(*amount))
                }),
                "after": after.filter(|_| bidder.discount.is_some()).map(money_eur),
                "gross": gross.map(money_eur),
                "side": if bidder.side_offers { "ja" } else { "nein" },
                "remark": breakable(&bidder.remark),
            })
        })
        .collect();
    json!({
        "header": view::header_with(
            &submission.header,
            "Niederschrift über die Angebotsöffnung",
            files,
            &[
                ("kopftext", submission.header_text.trim().to_string()),
                ("fusstext", submission.footer_text.trim().to_string()),
            ],
        ),
        "facts": submission.facts.iter().map(|(label, text)| json!({"label": label, "text": breakable(text)})).collect::<Vec<_>>(),
        "rows": rows,
        "vat_label": format!("brutto inkl. {} MwSt.", crate::format::percent(vat)),
        "notes": submission.notes,
        "signatures": submission.signatures,
        "header_text": submission.header_text.trim(),
        "footer_text": submission.footer_text.trim(),
    })
}
