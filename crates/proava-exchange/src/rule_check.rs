//! GAEB-Fachregeln R1–R13 (`proava_gaeb::rules`) on the files proAVA
//! writes and on the bids it reads.

use proava_application::GaebExport;
use proava_domain::Project;
use proava_gaeb::{datan, rules, xml, GaebDocument, Phase};

pub use proava_gaeb::rules::{Finding as RuleFinding, Severity as RuleSeverity};

use crate::files::GaebFormat;
use crate::lv_to_gaeb;

/// GAEB phase of an export.
pub fn export_phase(kind: GaebExport) -> Phase {
    match kind {
        GaebExport::Description => Phase::Dp81,
        GaebExport::RequestForQuotation => Phase::Dp83,
        GaebExport::PricedLv => Phase::Dp82,
    }
}

/// Fachregeln of the GAEB DA XML file the export would write for `kind`
/// (written in memory and read back, so the check sees exactly the file).
pub fn export_rule_findings(project: &Project, kind: GaebExport) -> Vec<RuleFinding> {
    let phase = export_phase(kind);
    let (document, _) = lv_to_gaeb(project, phase);
    match xml::write(&document, phase) {
        Ok(text) => file_findings(GaebFormat::Xml, text.as_bytes(), phase),
        // The export itself reports why it cannot write.
        Err(_) => Vec::new(),
    }
}

/// Fachregeln of the bytes of a written GAEB file.
pub(crate) fn file_findings(format: GaebFormat, bytes: &[u8], phase: Phase) -> Vec<RuleFinding> {
    let (mut findings, document) = match format {
        GaebFormat::Xml => (rules::check_xml(bytes), xml::read(bytes).ok()),
        GaebFormat::Datan => (
            rules::check_datan(bytes),
            datan::read(bytes).ok().map(|report| report.document),
        ),
    };
    if let Some(document) = document {
        findings.extend(rules::check(&document, phase, None));
    }
    findings
}

/// R11 for a bid against the request for quotation as proAVA writes it
/// from the project – without what the bid match already reports
/// (unknown OZ, quantities, project name).
pub(crate) fn bid_rule_findings(project: &Project, bid: &GaebDocument) -> Vec<RuleFinding> {
    let (request, _) = lv_to_gaeb(project, Phase::Dp83);
    let request = xml::as_written(&request, Phase::Dp83);
    rules::check_bid(bid, &request)
        .into_iter()
        .filter(|finding| {
            !matches!(
                finding.id,
                "R11.unknown_ordinal" | "R11.quantity" | "R11.project"
            )
        })
        .collect()
}

/// One line for a message list: "Fachregel R9 (Fehler): … [Fundstelle]".
pub fn rule_line(finding: &RuleFinding) -> String {
    format!(
        "Fachregel {} ({}): {} [{}]",
        finding.code(),
        finding.severity.label(),
        finding.text,
        finding.reference
    )
}
