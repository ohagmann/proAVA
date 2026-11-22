//! Fachregeln R1–R13: what the GAEB Fachdokumentation DA XML 3.3 (Ausgabe
//! 2023-01) demands beyond the XSD, plus the checks of the BVBS
//! certification and of the e-Vergabe platforms. Overview and sources:
//! `docs/FACHLICHES.md`.
//!
//! [`check`] looks at a read exchange model (DA XML or GAEB 90) in a phase,
//! for a bid (84/85) optionally against the request for quotation (R11).
//! [`check_xml`] and [`check_datan`] look at the bytes of a file for what the
//! model does not carry (ComplTSA/ComplTSB, GAEBInfo, record numbers …).
//!
//! ```
//! use proava_gaeb::{rules, GaebDocument, Phase};
//! # fn demo(doc: &GaebDocument) {
//! for finding in rules::check(doc, Phase::Dp83, None) {
//!     println!("{finding}");
//! }
//! # }
//! ```
//!
//! Not checkable on the model (the readers do not keep it): QtySplit, CONo,
//! SumDescr, LotGrp, the VAT rate per item (Zuschlag nur auf Positionen mit
//! gleichem USt-Satz).

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::model::*;
use crate::xml::calc;
use crate::xml::dom::{self, Element};

#[cfg(test)]
mod tests;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// A rule of the Fachdokumentation is violated.
    Error,
    Warning,
    Hint,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "Fehler",
            Severity::Warning => "Warnung",
            Severity::Hint => "Hinweis",
        }
    }
}

/// One finding of a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Identifier of the single check, e.g. "R3.duplicate"; the part before
    /// the dot is the rule (R1–R13).
    pub id: &'static str,
    pub severity: Severity,
    /// Where the rule stands, e.g. "FD 3.3 11.2.3 BoQInfo 2" (FD =
    /// Fachdokumentation GAEB DA XML 3.3, Ausgabe 2023-01).
    pub reference: &'static str,
    /// OZ of the title or item concerned, pieces joined with ".".
    pub ordinal: Option<String>,
    /// Message for the user (German).
    pub text: String,
}

impl Finding {
    /// "R3" for "R3.duplicate".
    pub fn code(&self) -> &str {
        self.id.split('.').next().unwrap_or(self.id)
    }

    /// Rule number 1–13.
    pub fn rule(&self) -> u8 {
        self.code()
            .trim_start_matches('R')
            .parse()
            .unwrap_or_default()
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {} [{}]",
            self.code(),
            self.severity.label(),
            self.text,
            self.reference
        )
    }
}

/// Number of errors, warnings and hints.
pub fn counts(findings: &[Finding]) -> [usize; 3] {
    let mut counts = [0; 3];
    for finding in findings {
        counts[finding.severity as usize] += 1;
    }
    counts
}

/// All rules R1–R13 that can be checked on the model, for the document in
/// `phase` (normally `doc.phase`). `request` is the Angebotsaufforderung
/// (X83) of a bid (84/85) for R11. Findings: errors first, then by rule, in
/// document order.
pub fn check(doc: &GaebDocument, phase: Phase, request: Option<&GaebDocument>) -> Vec<Finding> {
    let mut checker = Checker::new(doc, phase);
    checker.mask();
    checker.ordinals();
    checker.order();
    checker.depth(&doc.award.boq.body, None);
    checker.alternatives();
    checker.totals_by_kind();
    checker.surcharges(&doc.award.boq.body, &mut Vec::new());
    checker.sums();
    checker.items();
    checker.descriptions();
    if let (Some(request), Phase::Dp84 | Phase::Dp85) = (request, phase) {
        checker.findings.extend(check_bid(doc, request));
    }
    checker.tender();
    let mut findings = checker.findings;
    sort(&mut findings);
    findings
}

/// R11 only: the bid (X84/X85) against its Angebotsaufforderung (X83).
pub fn check_bid(bid: &GaebDocument, request: &GaebDocument) -> Vec<Finding> {
    let mut out = Out::default();
    let bid_mask = &bid.award.boq.breakdown;
    let request_mask = &request.award.boq.breakdown;
    if !bid_mask.is_empty() && !request_mask.is_empty() {
        let shape = |mask: &[BreakdownLevel]| -> Vec<(BreakdownKind, u8, bool)> {
            mask.iter()
                .map(|level| {
                    let numeric = level.numeric && level.kind != BreakdownKind::Index;
                    (level.kind, level.length, numeric)
                })
                .collect()
        };
        if shape(bid_mask) != shape(request_mask) {
            out.push(
                "R11.breakdown",
                Severity::Error,
                "FD 3.3 4.6.19; BVBS 3.3 Bauausführung 2.5",
                None,
                format!(
                    "Die OZ-Maske des Angebots ({}) weicht von der Angebotsaufforderung ({}) ab.",
                    mask_text(bid_mask),
                    mask_text(request_mask)
                ),
            );
        }
    }
    let requested: HashMap<String, &Item> = request
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, item)| (item_key(&pieces), item))
        .collect();
    let mut unknown = Vec::new();
    for (pieces, item) in bid.award.boq.items() {
        let oz = pieces.join(".");
        let Some(source) = requested.get(&item_key(&pieces)) else {
            unknown.push(oz);
            continue;
        };
        for complement in item.complements.iter().filter(|c| c.kind != "Owner") {
            let marker = complement.marker.trim();
            match source
                .complements
                .iter()
                .find(|c| c.marker.trim() == marker)
            {
                None => out.push(
                    "R11.complement",
                    Severity::Error,
                    "FD 3.3 4.6.16; BVBS 3.3 AVA 1.25",
                    Some(oz.clone()),
                    format!(
                        "OZ {oz}: Textergänzung {marker} ist in der Angebotsaufforderung nicht vorgesehen."
                    ),
                ),
                Some(found) if found.kind == "Owner" => out.push(
                    "R11.complement",
                    Severity::Error,
                    "FD 3.3 4.6.16",
                    Some(oz.clone()),
                    format!(
                        "OZ {oz}: Textergänzung {marker} ist eine Textergänzung des Auftraggebers, der Bieter darf sie nicht ausfüllen."
                    ),
                ),
                Some(_) => {}
            }
        }
        match item.qty {
            Some(qty) if !source.qty_to_be_determined && source.qty.is_some_and(|q| q != qty) => {
                out.push(
                    "R11.quantity",
                    Severity::Error,
                    "FD 3.3 4.6.19; Regeln für DA84 (3.1)",
                    Some(oz.clone()),
                    format!(
                        "OZ {oz}: Menge {} statt {} – im Angebot ist nur die freie Menge änderbar.",
                        plain(qty),
                        source.qty.map(plain).unwrap_or_default()
                    ),
                );
            }
            None if source.qty_to_be_determined
                && !item.not_offered
                && item.unit_price.is_some() =>
            {
                out.push(
                    "R11.free_qty",
                    Severity::Warning,
                    "FD 3.3 4.5.5, 4.6.11",
                    Some(oz.clone()),
                    format!("OZ {oz}: freie Menge gefordert, das Angebot nennt keine Menge."),
                );
            }
            _ => {}
        }
    }
    if !unknown.is_empty() {
        let first = unknown[0].clone();
        out.push(
            "R11.unknown_ordinal",
            Severity::Error,
            "FD 3.3 4.6.19",
            Some(first),
            format!(
                "{} OZ des Angebots fehlen in der Angebotsaufforderung: {}",
                unknown.len(),
                short_list(&unknown)
            ),
        );
    }
    let differs = |a: &str, b: &str| {
        !a.trim().is_empty() && !b.trim().is_empty() && !a.trim().eq_ignore_ascii_case(b.trim())
    };
    if differs(&bid.award.award_no, &request.award.award_no) {
        out.push(
            "R11.award_no",
            Severity::Warning,
            "FD 3.3 4.6.1",
            None,
            format!(
                "Vergabenummer „{}“ weicht ab von der Angebotsaufforderung „{}“.",
                bid.award.award_no.trim(),
                request.award.award_no.trim()
            ),
        );
    }
    if differs(&bid.project.currency, &request.project.currency) {
        out.push(
            "R11.currency",
            Severity::Warning,
            "FD 3.3 4.6.18",
            None,
            format!(
                "Währung „{}“ weicht ab von der Angebotsaufforderung „{}“.",
                bid.project.currency.trim(),
                request.project.currency.trim()
            ),
        );
    }
    if differs(&bid.project.name, &request.project.name)
        || differs(&bid.award.boq.name, &request.award.boq.name)
    {
        out.push(
            "R11.project",
            Severity::Hint,
            "FD 3.3 4.6.19",
            None,
            format!(
                "Projekt/LV „{} / {}“ weicht ab von der Angebotsaufforderung „{} / {}“.",
                bid.project.name.trim(),
                bid.award.boq.name.trim(),
                request.project.name.trim(),
                request.award.boq.name.trim()
            ),
        );
    }
    out.0
}

/// Rules on the bytes of a GAEB DA XML file (what the model does not keep):
/// GAEBInfo and namespace (R12), ComplTSA/ComplTSB and Kurztext complements
/// (R10), complements in Hinweistexten of an X83 (R13).
pub fn check_xml(bytes: &[u8]) -> Vec<Finding> {
    let mut out = Out::default();
    let Ok(root) = dom::parse(&dom::decode(bytes)) else {
        return out.0;
    };
    let info = root.child("GAEBInfo");
    let text = |name: &str| {
        info.and_then(|info| info.child_text(name))
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let version = text("Version");
    let version_date = text("VersDate");
    let program = text("ProgSystem");
    let namespace = root.attr("xmlns").unwrap_or_default().trim().to_string();
    let phase = root
        .child("Award")
        .and_then(|award| award.child_text("DP"))
        .unwrap_or_default()
        .trim()
        .to_string();

    const DATES: [(&str, &[&str]); 4] = [
        ("3.3", &["2021-05", "2023-01"]),
        ("3.2", &["2013-10"]),
        ("3.1", &["2007-06", "2007-11", "2009-12", "2010-09"]),
        ("3.0", &["2004-05", "2004-07"]),
    ];
    match DATES.iter().find(|(known, _)| *known == version) {
        None => out.push(
            "R12.version",
            Severity::Warning,
            "GAEB DA XML 3.0–3.3; BVBS 3.3 AVA 2.2",
            None,
            format!("Version „{version}“ ist keine GAEB-DA-XML-Version 3.0 bis 3.3."),
        ),
        Some((_, dates)) if !dates.contains(&version_date.as_str()) => out.push(
            "R12.version_date",
            Severity::Warning,
            "BVBS 3.3 Texterstellung 2.13",
            None,
            format!(
                "VersDate „{version_date}“ passt nicht zur Version {version} (erwartet {}).",
                dates.join(" oder ")
            ),
        ),
        Some(_) => {}
    }
    if program.is_empty() || !program.chars().any(|ch| ch.is_ascii_digit()) {
        out.push(
            "R12.program",
            Severity::Warning,
            "BVBS 3.3 Texterstellung 2.13, AVA 2.2",
            None,
            format!("ProgSystem „{program}“ nennt nicht Name und Version des Programms."),
        );
    }
    let expected = match version.as_str() {
        "3.2" | "3.3" => Some(format!(
            "http://www.gaeb.de/GAEB_DA_XML/DA{phase}/{version}"
        )),
        "3.0" | "3.1" => Some("http://www.gaeb.de/GAEB_DA_XML/200407".to_string()),
        _ => None,
    };
    if let Some(expected) = expected.filter(|expected| *expected != namespace) {
        out.push(
            "R12.namespace",
            Severity::Warning,
            "Schemadateien GAEB DA XML (targetNamespace)",
            None,
            format!("Namensraum „{namespace}“ passt nicht zu Version {version} und Phase {phase} (erwartet {expected})."),
        );
    }

    if let Some(body) = root.path(&["Award", "BoQ", "BoQBody"]) {
        raw_body(body, &mut Vec::new(), &phase, &mut out);
    }
    let mut findings = out.0;
    sort(&mut findings);
    findings
}

/// Rules on the lines of a GAEB 90 file (R13): record numbers from 1
/// without gaps (columns 75–80), first record 00/T0, last 99/T9, no empty
/// lines, 80 characters per line.
pub fn check_datan(bytes: &[u8]) -> Vec<Finding> {
    const SOURCE: &str = "GAEB 90 Regelwerk; T&T-Handbuch 2.7; RIB e-Vergabe";
    let mut out = Out::default();
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    while lines
        .last()
        .is_some_and(|line| line.trim_matches(['\u{1a}', ' ']).is_empty())
    {
        lines.pop();
    }
    let mut empty = Vec::new();
    let mut length = Vec::new();
    let mut numbering: Option<(usize, String)> = None;
    let mut wrong_numbers = 0;
    let mut expected = 1usize;
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        if line.trim().is_empty() {
            empty.push(number);
            continue;
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() != 80 {
            length.push(number);
        }
        let field: String = chars.iter().skip(74).take(6).collect();
        if field.trim().parse::<usize>().ok() != Some(expected) {
            wrong_numbers += 1;
            numbering.get_or_insert((number, field.trim().to_string()));
        }
        expected += 1;
    }
    let records: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let kind = |line: &str| line.chars().take(2).collect::<String>();
    if let Some(first) = records.first() {
        if !matches!(kind(first).as_str(), "00" | "T0") {
            out.push(
                "R13.datan_start",
                Severity::Warning,
                SOURCE,
                None,
                format!(
                    "Die Datei beginnt mit Satzart „{}“ statt 00 oder T0.",
                    kind(first)
                ),
            );
        }
    }
    if let Some(last) = records.last() {
        if !matches!(kind(last).as_str(), "99" | "T9") {
            out.push(
                "R13.datan_end",
                Severity::Warning,
                SOURCE,
                None,
                format!(
                    "Die Datei endet mit Satzart „{}“ statt 99 (LV-Ende).",
                    kind(last)
                ),
            );
        }
    }
    if let Some((line, field)) = numbering {
        out.push(
            "R13.datan_numbering",
            Severity::Warning,
            SOURCE,
            None,
            format!(
                "Satznummern (Spalte 75–80) nicht lückenlos ab 1: {wrong_numbers} Sätze, erster in Zeile {line} („{field}“)."
            ),
        );
    }
    if !empty.is_empty() {
        out.push(
            "R13.datan_empty_line",
            Severity::Warning,
            SOURCE,
            None,
            format!("{} Leerzeile(n), erste in Zeile {}.", empty.len(), empty[0]),
        );
    }
    if !length.is_empty() {
        out.push(
            "R13.datan_line_length",
            Severity::Warning,
            SOURCE,
            None,
            format!(
                "{} Zeile(n) nicht 80 Zeichen lang, erste Zeile {}.",
                length.len(),
                length[0]
            ),
        );
    }
    out.0
}

// ------------------------------------------------------------ internals ----

#[derive(Default)]
struct Out(Vec<Finding>);

impl Out {
    fn push(
        &mut self,
        id: &'static str,
        severity: Severity,
        reference: &'static str,
        ordinal: Option<String>,
        text: String,
    ) {
        self.0.push(Finding {
            id,
            severity,
            reference,
            ordinal,
            text,
        });
    }
}

fn sort(findings: &mut [Finding]) {
    findings.sort_by_key(|finding| (finding.severity, finding.rule()));
}

fn round2(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

fn plain(value: Decimal) -> String {
    value.normalize().to_string().replace('.', ",")
}

fn money(value: Decimal) -> String {
    format!("{:.2}", round2(value)).replace('.', ",")
}

fn short_list(list: &[String]) -> String {
    const LIMIT: usize = 8;
    let shown = list
        .iter()
        .take(LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if list.len() > LIMIT {
        format!("{shown} … (+{})", list.len() - LIMIT)
    } else {
        shown
    }
}

fn mask_text(mask: &[BreakdownLevel]) -> String {
    mask.iter()
        .map(|level| {
            let kind = match level.kind {
                BreakdownKind::Lot => "Los",
                BreakdownKind::Level => "Stufe",
                BreakdownKind::Item => "Position",
                BreakdownKind::Index => "Index",
            };
            format!("{kind} {}", level.length)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ordinal pieces without leading zeros, upper case – the same item
/// whether a file pads its numbers or not.
fn item_key(pieces: &[String]) -> String {
    pieces
        .iter()
        .map(|piece| {
            let piece = piece.trim();
            let stripped = piece.trim_start_matches('0');
            if stripped.is_empty() && !piece.is_empty() {
                "0".to_string()
            } else {
                stripped.to_uppercase()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Sort rank of a character in an OZ (FD 3.3 4.2.1): space, digits, upper
/// case letters, underscore, lower case letters.
fn rank(ch: char) -> u32 {
    match ch {
        ' ' => 0,
        '0'..='9' => 1 + (ch as u32 - '0' as u32),
        'A'..='Z' => 11 + (ch as u32 - 'A' as u32),
        '_' => 37,
        'a'..='z' => 38 + (ch as u32 - 'a' as u32),
        other => 100 + other as u32,
    }
}

fn compare_piece(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.trim(), b.trim());
    let digits = |text: &str| !text.is_empty() && text.chars().all(|ch| ch.is_ascii_digit());
    if digits(a) && digits(b) {
        let (x, y) = (a.trim_start_matches('0'), b.trim_start_matches('0'));
        return x.len().cmp(&y.len()).then_with(|| x.cmp(y));
    }
    a.chars().map(rank).cmp(b.chars().map(rank))
}

fn compare_pieces(a: &[String], b: &[String]) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        match compare_piece(x, y) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

fn alternative_is_base(alternative: &Alternative) -> bool {
    alternative.serial.trim().trim_start_matches('0').is_empty()
}

fn group_key(alternative: &Alternative) -> String {
    let group = alternative.group.trim();
    let stripped = group.trim_start_matches('0');
    if stripped.is_empty() {
        group.to_string()
    } else {
        stripped.to_string()
    }
}

#[derive(Clone, Copy)]
enum Node<'a> {
    Category(&'a Category),
    Item(&'a Item),
    Remark(&'a Remark),
    Perf(&'a PerfDescr),
}

struct Visit<'a> {
    node: Node<'a>,
    /// Ordinal pieces (title pieces, item, index) – empty for remarks.
    pieces: Vec<String>,
    /// Number of titles above.
    depth: usize,
}

impl Visit<'_> {
    fn oz(&self) -> String {
        self.pieces.join(".")
    }
}

fn visits(nodes: &[BoqNode]) -> Vec<Visit<'_>> {
    fn walk<'a>(nodes: &'a [BoqNode], path: &mut Vec<String>, out: &mut Vec<Visit<'a>>) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    out.push(Visit {
                        node: Node::Category(category),
                        pieces: path.clone(),
                        depth: path.len() - 1,
                    });
                    walk(&category.body, path, out);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    let mut pieces = path.clone();
                    pieces.push(item.rno_part.clone());
                    if !item.rno_index.is_empty() {
                        pieces.push(item.rno_index.clone());
                    }
                    out.push(Visit {
                        node: Node::Item(item),
                        pieces,
                        depth: path.len(),
                    });
                }
                BoqNode::Remark(remark) => out.push(Visit {
                    node: Node::Remark(remark),
                    pieces: Vec::new(),
                    depth: path.len(),
                }),
                BoqNode::PerfDescr(perf) => out.push(Visit {
                    node: Node::Perf(perf),
                    pieces: Vec::new(),
                    depth: path.len(),
                }),
            }
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

struct Checker<'a> {
    doc: &'a GaebDocument,
    phase: Phase,
    visits: Vec<Visit<'a>>,
    /// Lot and title levels of the mask, outermost first.
    levels: Vec<&'a BreakdownLevel>,
    item_level: Option<&'a BreakdownLevel>,
    index_level: Option<&'a BreakdownLevel>,
    findings: Vec<Finding>,
}

impl<'a> Checker<'a> {
    fn new(doc: &'a GaebDocument, phase: Phase) -> Self {
        let breakdown = &doc.award.boq.breakdown;
        Self {
            doc,
            phase,
            visits: visits(&doc.award.boq.body),
            levels: breakdown
                .iter()
                .filter(|level| matches!(level.kind, BreakdownKind::Lot | BreakdownKind::Level))
                .collect(),
            item_level: breakdown
                .iter()
                .find(|level| level.kind == BreakdownKind::Item),
            index_level: breakdown
                .iter()
                .find(|level| level.kind == BreakdownKind::Index),
            findings: Vec::new(),
        }
    }

    fn push(
        &mut self,
        id: &'static str,
        severity: Severity,
        reference: &'static str,
        ordinal: Option<String>,
        text: String,
    ) {
        self.findings.push(Finding {
            id,
            severity,
            reference,
            ordinal,
            text,
        });
    }

    fn has_mask(&self) -> bool {
        !self.doc.award.boq.breakdown.is_empty()
    }

    /// The bid short form (84/85) carries no item kinds and no mask in
    /// GAEB DA XML 3.1.
    fn is_bid(&self) -> bool {
        matches!(self.phase, Phase::Dp84 | Phase::Dp85)
    }

    fn priced(&self) -> bool {
        self.phase.has_prices()
    }

    // R1 – BoQBkdn (FD 3.3 11.2.3 BoQInfo 1–10, BoQBkdn 1–2)
    fn mask(&mut self) {
        let breakdown = &self.doc.award.boq.breakdown;
        if breakdown.is_empty() {
            if !self.is_bid() {
                self.push(
                    "R1.mask_missing",
                    Severity::Error,
                    "FD 3.3 11.2.3 BoQInfo 5",
                    None,
                    "Die OZ-Maske (BoQBkdn) fehlt.".to_string(),
                );
            }
            return;
        }
        let position = |kind| breakdown.iter().position(|level| level.kind == kind);
        let count = |kind| breakdown.iter().filter(|level| level.kind == kind).count();
        let items = count(BreakdownKind::Item);
        if items != 1 {
            self.push(
                "R1.item",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 5",
                None,
                format!("Die OZ-Maske hat {items} Positionsstufen statt genau einer."),
            );
        }
        if count(BreakdownKind::Lot) > 1 {
            self.push(
                "R1.lot",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 4",
                None,
                "Die OZ-Maske hat mehr als eine Stufe „Los“.".to_string(),
            );
        }
        if position(BreakdownKind::Lot).is_some_and(|index| index > 0) {
            self.push(
                "R1.lot",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 3",
                None,
                "Die Stufe „Los“ steht nicht an erster Stelle der OZ-Maske.".to_string(),
            );
        }
        let item_at = position(BreakdownKind::Item).unwrap_or(breakdown.len());
        let before = breakdown[..item_at]
            .iter()
            .filter(|level| matches!(level.kind, BreakdownKind::Lot | BreakdownKind::Level))
            .count();
        if before > 5 {
            self.push(
                "R1.levels",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 6; FD 3.3 4.2.1",
                None,
                format!("Die OZ-Maske hat {before} Hierarchiestufen (Los eingeschlossen), erlaubt sind 5."),
            );
        }
        if breakdown[item_at.min(breakdown.len())..]
            .iter()
            .any(|level| matches!(level.kind, BreakdownKind::Lot | BreakdownKind::Level))
        {
            self.push(
                "R1.levels",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 6",
                None,
                "In der OZ-Maske steht eine Hierarchiestufe nach der Positionsstufe.".to_string(),
            );
        }
        let indexes = count(BreakdownKind::Index);
        if indexes > 1 {
            self.push(
                "R1.index",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 8",
                None,
                format!("Die OZ-Maske hat {indexes} Indexstufen, erlaubt ist eine."),
            );
        }
        if position(BreakdownKind::Index).is_some_and(|index| index < item_at) {
            self.push(
                "R1.index",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 7",
                None,
                "Die Indexstufe steht vor der Positionsstufe.".to_string(),
            );
        }
        if breakdown
            .iter()
            .any(|level| level.kind == BreakdownKind::Index && level.length != 1)
        {
            self.push(
                "R1.index",
                Severity::Error,
                "FD 3.3 11.2.3 BoQBkdn 2",
                None,
                "Die Indexstufe ist nicht einstellig (Length = 1).".to_string(),
            );
        }
        let total: u32 = breakdown.iter().map(|level| level.length as u32).sum();
        if total > 14 {
            self.push(
                "R1.length",
                Severity::Error,
                "FD 3.3 11.2.3 BoQInfo 2; FD 3.3 4.2.1",
                None,
                format!("Die OZ-Maske ist {total} Stellen lang, erlaubt sind 14."),
            );
        }
        let labelled: Vec<&str> = breakdown
            .iter()
            .filter(|level| level.kind != BreakdownKind::Level && !level.label.trim().is_empty())
            .map(|level| match level.kind {
                BreakdownKind::Lot => "Los",
                BreakdownKind::Item => "Position",
                _ => "Index",
            })
            .collect();
        if !labelled.is_empty() {
            self.push(
                "R1.label",
                Severity::Error,
                "FD 3.3 11.2.3 BoQBkdn 1",
                None,
                format!(
                    "Bezeichnung an Stufe {} – erlaubt nur an LV-Stufen (BoQLevel).",
                    labelled.join(", ")
                ),
            );
        }
        let declared = self.doc.award.boq.up_component_labels.len();
        if declared == 0 {
            let first = self
                .visits
                .iter()
                .find(|visit| {
                    matches!(visit.node, Node::Item(item) if item.up_components.iter().any(Option::is_some))
                })
                .map(Visit::oz);
            if let Some(oz) = first {
                self.push(
                    "R1.up_components",
                    Severity::Error,
                    "FD 3.3 11.2.3 BoQInfo 9",
                    Some(oz.clone()),
                    format!("Positionen haben Einheitspreisanteile (z. B. OZ {oz}), die Aufgliederung (NoUPComps) ist nicht angegeben."),
                );
            }
        }
    }

    // R2 – OZ pieces against the mask (FD 3.3 4.2.1)
    fn ordinals(&mut self) {
        if !self.has_mask() {
            return;
        }
        let mut problems = Vec::new();
        for visit in &self.visits {
            match visit.node {
                Node::Category(category) => {
                    if let Some(level) = self.levels.get(visit.depth) {
                        let what = if level.kind == BreakdownKind::Lot {
                            "Los"
                        } else {
                            "Titel"
                        };
                        problems.extend(piece_problems(
                            &visit.oz(),
                            &category.rno_part,
                            level,
                            what,
                        ));
                    }
                }
                Node::Item(item) => {
                    let oz = visit.oz();
                    if let Some(level) = self.item_level {
                        problems.extend(piece_problems(&oz, &item.rno_part, level, "Position"));
                    }
                    let index = item.rno_index.trim();
                    if !index.is_empty() {
                        if self.index_level.is_none() {
                            problems.push((
                                "R2.index",
                                Severity::Error,
                                oz.clone(),
                                format!(
                                    "OZ {oz}: Index „{index}“, die OZ-Maske hat keine Indexstufe."
                                ),
                            ));
                        } else if index.chars().count() != 1
                            || !index.chars().all(|ch| ch.is_ascii_alphanumeric())
                        {
                            problems.push((
                                "R2.index",
                                Severity::Error,
                                oz.clone(),
                                format!(
                                    "OZ {oz}: Index „{index}“ ist nicht einstellig alphanumerisch."
                                ),
                            ));
                        }
                    }
                }
                Node::Remark(_) | Node::Perf(_) => {}
            }
        }
        for (id, severity, oz, text) in problems {
            let reference = if id == "R2.zero" {
                "FD 3.3 4.2.1; BVBS 3.3 Bauausführung 1.12/1.13 (Auslegung offen)"
            } else {
                "FD 3.3 4.2.1"
            };
            self.push(id, severity, reference, Some(oz), text);
        }
    }

    // R3 – OZ unique and ascending (FD 3.3 4.2.1)
    fn order(&mut self) {
        let mut seen: HashSet<(bool, Vec<String>)> = HashSet::new();
        let mut previous: Option<Vec<String>> = None;
        let mut findings = Vec::new();
        for visit in &self.visits {
            let is_item = match visit.node {
                Node::Item(_) => true,
                Node::Category(_) => false,
                _ => continue,
            };
            let key: Vec<String> = visit
                .pieces
                .iter()
                .map(|piece| item_key(std::slice::from_ref(piece)))
                .collect();
            let oz = visit.oz();
            if !seen.insert((is_item, key)) {
                findings.push((
                    "R3.duplicate",
                    Severity::Error,
                    oz.clone(),
                    format!("OZ {oz} kommt mehrfach vor."),
                ));
            } else if let Some(previous) = previous
                .as_ref()
                .filter(|previous| compare_pieces(&visit.pieces, previous) != Ordering::Greater)
            {
                findings.push((
                    "R3.order",
                    Severity::Warning,
                    oz.clone(),
                    format!(
                        "OZ {oz} steht nach {} – die OZ müssen im LV aufsteigend sein.",
                        previous.join(".")
                    ),
                ));
            }
            previous = Some(visit.pieces.clone());
        }
        for (id, severity, oz, text) in findings {
            self.push(id, severity, "FD 3.3 4.2.1", Some(oz), text);
        }
    }

    // R4 – depth of the titles, items in the outline (FD 3.3 4.2.2, 4.3.4)
    fn depth(&mut self, nodes: &'a [BoqNode], parent: Option<String>) {
        if !self.has_mask() {
            return;
        }
        let depth = parent.as_ref().map_or(0, |oz| oz.split('.').count());
        let has_category = nodes
            .iter()
            .any(|node| matches!(node, BoqNode::Category(_)));
        let has_item = nodes.iter().any(|node| matches!(node, BoqNode::Item(_)));
        if has_category && has_item {
            let text = match &parent {
                Some(oz) => format!("Titel {oz} enthält Untertitel und Positionen nebeneinander."),
                None => "Das LV enthält auf oberster Ebene Titel und Positionen nebeneinander."
                    .to_string(),
            };
            self.push(
                "R4.mixed",
                Severity::Error,
                "FD 3.3 4.3.4; Schema tgBoQBody",
                parent.clone(),
                text,
            );
        }
        if has_item && depth == 0 && !self.levels.is_empty() {
            let oz = nodes.iter().find_map(|node| match node {
                BoqNode::Item(item) => Some(item.rno_part.clone()),
                _ => None,
            });
            self.push(
                "R4.outside",
                Severity::Error,
                "FD 3.3 4.3.4",
                oz.clone(),
                format!(
                    "Position {} steht außerhalb der Gliederung – die oberste Stufe darf nicht unbelegt bleiben.",
                    oz.unwrap_or_default()
                ),
            );
        }
        for node in nodes {
            if let BoqNode::Category(category) = node {
                let oz = match &parent {
                    Some(parent) => format!("{parent}.{}", category.rno_part),
                    None => category.rno_part.clone(),
                };
                if depth >= self.levels.len() {
                    self.push(
                        "R4.too_deep",
                        Severity::Error,
                        "FD 3.3 4.2.2",
                        Some(oz.clone()),
                        format!(
                            "Titel {oz} liegt auf Stufe {}, die OZ-Maske hat {} Hierarchiestufen.",
                            depth + 1,
                            self.levels.len()
                        ),
                    );
                    continue;
                }
                self.depth(&category.body, Some(oz));
            }
        }
    }

    // R5 – Grund- und Wahlpositionen/-bereiche (FD 3.3 4.5.2)
    fn alternatives(&mut self) {
        if self.is_bid() {
            return;
        }
        // group → (first base OZ, first alternative OZ)
        let mut groups: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
        for visit in &self.visits {
            let alternative = match visit.node {
                Node::Item(item) => item.kind.alternative.as_ref(),
                Node::Category(category) => category.alternative.as_ref(),
                _ => None,
            };
            let Some(alternative) = alternative else {
                continue;
            };
            let entry = groups.entry(group_key(alternative)).or_default();
            let slot = if alternative_is_base(alternative) {
                &mut entry.0
            } else {
                &mut entry.1
            };
            slot.get_or_insert_with(|| visit.oz());
        }
        for (group, (base, alternative)) in groups {
            match (base, alternative) {
                (None, Some(oz)) => self.push(
                    "R5.without_base",
                    Severity::Error,
                    "FD 3.3 4.5.2; RIB e-Vergabe",
                    Some(oz.clone()),
                    format!("Wahlausführung {oz} (Gruppe {group}) ohne Grundausführung."),
                ),
                (Some(oz), None) => self.push(
                    "R5.without_alternative",
                    Severity::Error,
                    "FD 3.3 4.5.2",
                    Some(oz.clone()),
                    format!("Grundausführung {oz} (Gruppe {group}) ohne Wahlausführung."),
                ),
                _ => {}
            }
        }
    }

    // R6 – no total where the item kind shows none (FD 3.3 4.5.2, 4.5.3)
    fn totals_by_kind(&mut self) {
        if !self.priced() || self.is_bid() {
            return;
        }
        let mut found = Vec::new();
        for visit in &self.visits {
            match visit.node {
                Node::Item(item)
                    if item.total.is_some()
                        && !calc::shows_total(item)
                        && !calc::is_surcharge(item) =>
                {
                    let kind = if item.not_applicable {
                        "entfällt"
                    } else if item.kind.provisional == Some(Provisional::WithoutTotal) {
                        "Bedarfsposition ohne GB"
                    } else if item.kind.provisional.is_some() {
                        "Bedarfs- und Wahlposition"
                    } else {
                        "Wahlposition"
                    };
                    let oz = visit.oz();
                    found.push((
                        "R6.total",
                        Severity::Warning,
                        "FD 3.3 4.5.2, 4.5.3; BVBS 3.3 AVA 4.5",
                        oz.clone(),
                        format!("OZ {oz} ({kind}) trägt einen Gesamtbetrag – vorgesehen ist „Nur Einh.-Preis“."),
                    ));
                }
                Node::Category(category)
                    if calc::is_alternative_area(category)
                        && category.totals.as_ref().is_some_and(|totals| {
                            totals.total.is_some_and(|total| !total.is_zero())
                        }) =>
                {
                    let oz = visit.oz();
                    found.push((
                        "R6.area_total",
                        Severity::Hint,
                        "BVBS 3.3 AVA 4.5",
                        oz.clone(),
                        format!("Wahlbereich {oz} trägt eine Summe; sie zählt nicht zur LV-Summe."),
                    ));
                }
                _ => {}
            }
        }
        for (id, severity, reference, oz, text) in found {
            self.push(id, severity, reference, Some(oz), text);
        }
    }

    // R7 – Zuschlagspositionen (FD 3.3 11.2.3 MarkupItem 1–4, 4.6.14)
    fn surcharges(&mut self, nodes: &'a [BoqNode], path: &mut Vec<String>) {
        // Values of the ordinary items of this list (given total first).
        let entries: Vec<(String, &Item, Decimal)> = nodes
            .iter()
            .filter_map(|node| match node {
                BoqNode::Item(item) if !calc::is_surcharge(item) => Some((
                    calc::ordinal(path, item),
                    item,
                    if calc::counts_in_total(item) {
                        item.total
                            .or_else(|| calc::item_total(item))
                            .unwrap_or_default()
                    } else {
                        Decimal::ZERO
                    },
                )),
                _ => None,
            })
            .collect();
        let (mut seen, mut group_start, mut group_end) = (0usize, 0usize, 0usize);
        let mut last_was_surcharge = false;
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    self.surcharges(&category.body, path);
                    path.pop();
                }
                BoqNode::Item(item) if !calc::is_surcharge(item) => {
                    seen += 1;
                    last_was_surcharge = false;
                }
                BoqNode::Item(item) => {
                    if !last_was_surcharge {
                        group_start = group_end;
                        group_end = seen;
                    }
                    last_was_surcharge = true;
                    let oz = calc::ordinal(path, item);
                    self.surcharge(item, &oz, &entries, &entries[group_start..group_end]);
                }
                _ => {}
            }
        }
    }

    fn surcharge(
        &mut self,
        item: &Item,
        oz: &str,
        entries: &[(String, &Item, Decimal)],
        group: &[(String, &Item, Decimal)],
    ) {
        const MARKUP: &str = "FD 3.3 11.2.3 MarkupItem 1–4";
        let markup_type = item
            .surcharge
            .as_ref()
            .map(|surcharge| surcharge.markup_type.trim())
            .unwrap_or_default();
        let references = item
            .surcharge
            .as_ref()
            .map(|surcharge| surcharge.items.as_slice())
            .unwrap_or_default();
        let mut push = |id, severity, reference, text: String| {
            self.findings.push(Finding {
                id,
                severity,
                reference,
                ordinal: Some(oz.to_string()),
                text,
            })
        };
        match markup_type {
            "" => {}
            "ListInSubQty" if references.is_empty() => push(
                "R7.sub_qty",
                Severity::Error,
                MARKUP,
                format!("Zuschlagsposition {oz} (ListInSubQty) nennt keine bezuschlagten Positionen (MarkupSubQty)."),
            ),
            "IdentAsMark" | "AllInCat" if !references.is_empty() => push(
                "R7.sub_qty",
                Severity::Error,
                MARKUP,
                format!("Zuschlagsposition {oz} ({markup_type}) darf keine MarkupSubQty enthalten."),
            ),
            "IdentAsMark" if !group.iter().any(|(_, item, _)| item.kind.markup) => push(
                "R7.marked",
                Severity::Warning,
                MARKUP,
                format!("Zuschlagsposition {oz} (IdentAsMark): davor ist keine Position als „zu bezuschlagen“ gekennzeichnet."),
            ),
            "ListInSubQty" | "IdentAsMark" | "AllInCat" => {}
            other => push(
                "R7.type",
                Severity::Error,
                MARKUP,
                format!("Zuschlagsposition {oz}: unbekannte Zuschlagsart „{other}“."),
            ),
        }
        if !self.priced() {
            return;
        }
        // The base only from the full LV (X84 carries no MarkupIt flags).
        let computed: Option<Decimal> =
            (!self.is_bid() && !markup_type.is_empty()).then(|| match markup_type {
                "ListInSubQty" => references
                    .iter()
                    .filter_map(|reference| {
                        let (_, entry, value) = entries
                            .iter()
                            .find(|(ordinal, _, _)| *ordinal == reference.ordinal)?;
                        Some(match (reference.sub_qty, entry.unit_price) {
                            (Some(qty), Some(price)) => round2(qty * price),
                            _ => *value,
                        })
                    })
                    .sum(),
                "IdentAsMark" => group
                    .iter()
                    .filter(|(_, item, _)| item.kind.markup)
                    .map(|(_, _, value)| *value)
                    .sum(),
                _ => group.iter().map(|(_, _, value)| *value).sum(),
            });
        let mut push = |id, text: String| {
            self.findings.push(Finding {
                id,
                severity: Severity::Error,
                reference: "FD 3.3 11.2.3 MarkupItem 1–3; BVBS 3.3 AVA 1.17–1.22",
                ordinal: Some(oz.to_string()),
                text,
            })
        };
        if let (Some(given), Some(computed)) = (item.markup_base, computed) {
            if !computed.is_zero() && round2(given) != round2(computed) {
                push(
                    "R7.base",
                    format!(
                        "Zuschlagsposition {oz}: zu bezuschlagender Betrag {} in der Datei, aus den Positionen gerechnet {}.",
                        money(given),
                        money(computed)
                    ),
                );
            }
        }
        let base = item
            .markup_base
            .or(computed.filter(|value| !value.is_zero()));
        if let (Some(base), Some(percent), Some(total)) = (base, item.markup_percent, item.total) {
            let expected = round2(base * percent / Decimal::ONE_HUNDRED);
            if round2(total) != expected {
                push(
                    "R7.total",
                    format!(
                        "Zuschlagsposition {oz}: Betrag {} ≠ {} × {} % = {}.",
                        money(total),
                        money(base),
                        plain(percent),
                        money(expected)
                    ),
                );
            }
        }
    }

    // R8 – sums, discounts, VAT (FD 3.3 11.2.3 Totals 1, 4.6.7, 4.6.17, 11.4.2)
    fn sums(&mut self) {
        if !self.priced() {
            return;
        }
        let mut found: Vec<(&'static str, Severity, &'static str, Option<String>, String)> =
            Vec::new();
        for visit in &self.visits {
            match visit.node {
                Node::Item(item) if !calc::is_surcharge(item) => {
                    let Some(total) = item.total else { continue };
                    if item.unit_price.is_none() || (item.qty.is_none() && !item.kind.lump_sum) {
                        continue;
                    }
                    if let Some(expected) = calc::item_total(item) {
                        if round2(total) != expected {
                            let oz = visit.oz();
                            found.push((
                                "R8.item_total",
                                Severity::Error,
                                "FD 3.3 4.6.17",
                                Some(oz.clone()),
                                format!(
                                    "OZ {oz}: Gesamtbetrag {} ≠ Menge × EP{} = {}.",
                                    money(total),
                                    if item.discount_percent.is_some() {
                                        " nach Nachlass"
                                    } else {
                                        ""
                                    },
                                    money(expected)
                                ),
                            ));
                        }
                    }
                }
                Node::Category(category) => {
                    let oz = visit.oz();
                    if let Some(totals) = &category.totals {
                        totals_problems(totals, &format!("Titel {oz}"), Some(&oz), &mut found);
                        if !self.is_bid() && totals.lump_sum.is_none() {
                            if let Some(total) = totals.total {
                                let expected =
                                    calc::nodes_total(&category.body, &mut visit.pieces.clone());
                                if round2(total) != round2(expected) {
                                    found.push((
                                        "R8.category_total",
                                        Severity::Error,
                                        "FD 3.3 4.6.17 Nr. 3 und 5",
                                        Some(oz.clone()),
                                        format!(
                                            "Titel {oz}: Summe {} ≠ Summe des Inhalts {}.",
                                            money(total),
                                            money(expected)
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(totals) = &self.doc.award.boq.totals {
            totals_problems(totals, "LV", None, &mut found);
            if !self.is_bid() && totals.lump_sum.is_none() {
                if let Some(total) = totals.total {
                    let expected = calc::nodes_total(&self.doc.award.boq.body, &mut Vec::new());
                    if round2(total) != round2(expected) {
                        found.push((
                            "R8.boq_total",
                            Severity::Error,
                            "FD 3.3 4.6.17 Nr. 5",
                            None,
                            format!(
                                "LV-Summe {} ≠ Summe der Titel und Positionen {}.",
                                money(total),
                                money(expected)
                            ),
                        ));
                    }
                }
            }
        }
        for (id, severity, reference, oz, text) in found {
            self.push(id, severity, reference, oz, text);
        }
    }

    // R9 – item rules (FD 3.3 11.2.3 Item 1–9, 4.6.4, 4.6.5, 4.6.11)
    fn items(&mut self) {
        let declared = self.doc.award.boq.up_component_labels.len();
        let mut found: Vec<(&'static str, &'static str, String, String)> = Vec::new();
        for visit in &self.visits {
            let Node::Item(item) = visit.node else {
                continue;
            };
            let oz = visit.oz();
            let surcharge = calc::is_surcharge(item);
            if !self.is_bid() {
                if item.qty.is_none()
                    && !item.qty_to_be_determined
                    && !item.kind.lump_sum
                    && !surcharge
                {
                    found.push((
                        "R9.qty",
                        "FD 3.3 11.2.3 Item 2",
                        oz.clone(),
                        format!("OZ {oz} ohne Menge – erlaubt nur bei freier Menge oder Pauschalposition."),
                    ));
                }
                if item.unit.trim().is_empty() && !surcharge {
                    found.push((
                        "R9.unit",
                        "FD 3.3 11.2.3 Item 7",
                        oz.clone(),
                        format!("OZ {oz} ohne Mengeneinheit – sie darf nur in X84 fehlen."),
                    ));
                }
                if item.short_text.trim().is_empty() && item.long_text.is_empty() {
                    found.push((
                        "R9.description",
                        "FD 3.3 11.2.3 Item 7",
                        oz.clone(),
                        format!("OZ {oz} ohne Beschreibung – sie darf nur in X84 fehlen."),
                    ));
                }
            }
            if self.phase == Phase::Dp83 && item.qty_to_be_determined && item.qty.is_some() {
                found.push((
                    "R9.qty_tbd",
                    "FD 3.3 11.2.3 Item 3; FD 3.3 4.6.11",
                    oz.clone(),
                    format!("OZ {oz}: freie Menge gefordert, aber eine Menge vorgegeben."),
                ));
            }
            if item.kind.repetition_of.is_some() && item.kind.reference_description.is_some() {
                found.push((
                    "R9.references",
                    "FD 3.3 11.2.3 Item 1",
                    oz.clone(),
                    format!("OZ {oz} verweist zugleich auf eine Position und eine Ausführungsbeschreibung."),
                ));
            }
            let components = item.up_components.len();
            if declared > 0 && components > declared {
                found.push((
                    "R9.components",
                    "FD 3.3 11.2.3 BoQInfo 10; FD 3.3 4.6.5",
                    oz.clone(),
                    format!("OZ {oz} hat {components} Einheitspreisanteile, das LV sieht {declared} vor."),
                ));
            }
            if let Some(price) = item.unit_price {
                let parts: Vec<Decimal> = item.up_components.iter().flatten().copied().collect();
                let sum: Decimal = parts.iter().sum();
                if !parts.is_empty() && sum != price {
                    found.push((
                        "R9.components",
                        "FD 3.3 4.6.5",
                        oz.clone(),
                        format!(
                            "OZ {oz}: Summe der Einheitspreisanteile {} ≠ Einheitspreis {}.",
                            plain(sum),
                            plain(price)
                        ),
                    ));
                }
                if item.not_offered {
                    found.push((
                        "R9.not_offered",
                        "FD 3.3 4.6.4",
                        oz.clone(),
                        format!("OZ {oz} ist als „nicht angeboten“ gekennzeichnet und trägt doch einen Einheitspreis."),
                    ));
                }
            }
        }
        for (id, reference, oz, text) in found {
            self.push(id, Severity::Error, reference, Some(oz), text);
        }
    }

    // R10 – Ausführungsbeschreibungen, Textergänzungen (FD 3.3 11.2.3
    // PerfDescr 1–2, 4.6.16)
    fn descriptions(&mut self) {
        let numbers: Vec<String> = self
            .visits
            .iter()
            .filter_map(|visit| match visit.node {
                Node::Perf(perf) => Some(perf.number.trim().to_string()),
                _ => None,
            })
            .collect();
        let mut found: Vec<(&'static str, &'static str, Option<String>, String)> = Vec::new();
        let mut counted: HashMap<&str, usize> = HashMap::new();
        for number in &numbers {
            *counted.entry(number.as_str()).or_default() += 1;
        }
        let mut twice: Vec<&&str> = counted
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(number, _)| number)
            .collect();
        twice.sort();
        for number in twice {
            found.push((
                "R10.perf_no",
                "FD 3.3 11.2.3 PerfDescr 1",
                None,
                format!(
                    "Ausführungsbeschreibung {} gibt es mehrfach.",
                    crate::perf_mark(number)
                ),
            ));
        }
        let mut seen: HashSet<String> = HashSet::new();
        for visit in &self.visits {
            match visit.node {
                Node::Perf(perf) => {
                    seen.insert(perf.number.trim().to_string());
                }
                Node::Item(item) => {
                    let oz = visit.oz();
                    if let Some(number) = &item.kind.reference_description {
                        let number = number.trim();
                        if !seen.contains(number) && numbers.iter().any(|n| n == number) {
                            found.push((
                                "R10.perf_order",
                                "FD 3.3 11.2.3 PerfDescr 2",
                                Some(oz.clone()),
                                format!(
                                    "OZ {oz} bezieht sich auf Ausführungsbeschreibung {}, die erst danach steht.",
                                    crate::perf_mark(number)
                                ),
                            ));
                        }
                    }
                    let mut markers: HashSet<&str> = HashSet::new();
                    for complement in &item.complements {
                        let marker = complement.marker.trim();
                        if marker.is_empty() || !marker.chars().all(|ch| ch.is_ascii_digit()) {
                            found.push((
                                "R10.marker",
                                "FD 3.3 4.6.16",
                                Some(oz.clone()),
                                format!("OZ {oz}: Textergänzung ohne gültige Nummer (MarkLbl „{marker}“)."),
                            ));
                        } else if !markers.insert(marker) {
                            found.push((
                                "R10.marker",
                                "FD 3.3 4.6.16",
                                Some(oz.clone()),
                                format!("OZ {oz}: Textergänzung {marker} kommt mehrfach vor."),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        for (id, reference, oz, text) in found {
            self.push(id, Severity::Error, reference, oz, text);
        }
    }

    // R13 – e-Vergabe (RIB GAEB-Prüfer, nur Angebotsaufforderung 83)
    fn tender(&mut self) {
        if self.phase != Phase::Dp83 {
            return;
        }
        const RIB: &str = "RIB e-Vergabe GAEB-Prüfer";
        let mut found: Vec<(&'static str, Severity, Option<String>, String)> = Vec::new();
        let mut images = 0;
        for visit in &self.visits {
            match visit.node {
                Node::Item(item) => {
                    let oz = visit.oz();
                    images += image_count(&item.long_text);
                    if calc::is_surcharge(item) {
                        continue;
                    }
                    if item.long_text.is_empty() && item.sub_descriptions.is_empty() {
                        found.push((
                            "R13.long_text",
                            Severity::Warning,
                            Some(oz.clone()),
                            format!("OZ {oz} ohne Langtext – Vergabeplattformen lehnen das ab (VOB/A § 7)."),
                        ));
                    }
                    if item.qty.is_some_and(|qty| qty.is_zero()) {
                        found.push((
                            "R13.zero_qty",
                            Severity::Warning,
                            Some(oz.clone()),
                            format!("OZ {oz}: Menge 0."),
                        ));
                    }
                    let lines = item.short_text.lines().count();
                    if lines > 9 {
                        found.push((
                            "R13.short_text",
                            Severity::Warning,
                            Some(oz.clone()),
                            format!("OZ {oz}: Kurztext mit {lines} Zeilen, erlaubt sind 9."),
                        ));
                    }
                    if has_gap_marks(&item.long_text.to_plain()) {
                        found.push((
                            "R13.gap",
                            Severity::Hint,
                            Some(oz.clone()),
                            format!("OZ {oz}: Punkte/Striche im Langtext – vermutlich eine Bietertextergänzung ohne Kennzeichen."),
                        ));
                    }
                    if item.unit_price.is_some() {
                        found.push((
                            "R13.prices",
                            Severity::Warning,
                            Some(oz.clone()),
                            format!("OZ {oz} trägt in der Angebotsaufforderung einen Preis."),
                        ));
                    }
                }
                Node::Remark(remark) => images += image_count(&remark.text),
                _ => {}
            }
        }
        if images > 0 {
            found.push((
                "R13.images",
                Severity::Hint,
                None,
                format!(
                    "{images} {} im LV – Vergabeplattformen empfehlen, Bilder getrennt als PDF beizulegen.",
                    if images == 1 { "Bild" } else { "Bilder" }
                ),
            ));
        }
        for (id, severity, oz, text) in found {
            self.push(id, severity, RIB, oz, text);
        }
    }
}

/// Problems of one OZ piece against its mask level (R2).
fn piece_problems(
    oz: &str,
    piece: &str,
    level: &BreakdownLevel,
    what: &str,
) -> Vec<(&'static str, Severity, String, String)> {
    let mut out = Vec::new();
    let mut push = |id, severity, text: String| out.push((id, severity, oz.to_string(), text));
    let trimmed = piece.trim_end();
    if trimmed.trim().is_empty() {
        push(
            "R2.empty",
            Severity::Error,
            format!("{what} {oz}: Ordnungszahl fehlt."),
        );
        return out;
    }
    let length = trimmed.chars().count();
    if length > level.length as usize {
        push(
            "R2.length",
            Severity::Error,
            format!(
                "{what} {oz}: „{trimmed}“ hat {length} Stellen, die OZ-Maske erlaubt {}.",
                level.length
            ),
        );
    }
    let allowed = |ch: char| ch == ' ' || ch == '_' || ch.is_ascii_alphanumeric();
    if let Some(bad) = trimmed.chars().find(|ch| !allowed(*ch)) {
        push(
            "R2.characters",
            Severity::Error,
            format!("{what} {oz}: Zeichen „{bad}“ ist in der OZ nicht erlaubt (nur Ziffern, Buchstaben ohne Umlaute, Leerzeichen, Unterstrich)."),
        );
    } else if level.numeric && !trimmed.trim().chars().all(|ch| ch.is_ascii_digit()) {
        push(
            "R2.numeric",
            Severity::Error,
            format!("{what} {oz}: „{trimmed}“ ist nicht numerisch, die Stufe ist numerisch."),
        );
    }
    if trimmed.chars().all(|ch| matches!(ch, '0' | ' ' | '_')) {
        push(
            "R2.zero",
            Severity::Hint,
            format!("{what} {oz}: „{trimmed}“ besteht nur aus Nullen, Leerzeichen oder Unterstrichen – laut FD nur zusammen mit anderen Zeichen; die BVBS-Prüfdatei nutzt so eine Stufe (Auslegung offen)."),
        );
    }
    out
}

/// Findings of a Totals element (R8).
fn totals_problems(
    totals: &Totals,
    what: &str,
    oz: Option<&str>,
    out: &mut Vec<(&'static str, Severity, &'static str, Option<String>, String)>,
) {
    let ordinal = oz.map(str::to_string);
    let discount = totals.discount_percent.is_some() || totals.discount_amount.is_some();
    if discount && totals.total_after_discount.is_none() {
        out.push((
            "R8.after_discount",
            Severity::Error,
            "FD 3.3 11.2.3 Totals 1; FD 3.3 4.6.7",
            ordinal.clone(),
            format!("{what}: Nachlass ohne Summe nach Nachlass (TotAfterDisc)."),
        ));
    }
    if let (Some(total), Some(after)) = (totals.total, totals.total_after_discount) {
        let expected = if totals.discount_percent.is_some() || totals.discount_amount.is_some() {
            Some(calc::after_discount(total, Some(totals)))
        } else {
            None
        };
        if let Some(expected) = expected.filter(|expected| round2(after) != *expected) {
            out.push((
                "R8.after_discount",
                Severity::Error,
                "FD 3.3 4.6.17",
                ordinal.clone(),
                format!(
                    "{what}: Summe nach Nachlass {} ≠ gerechnet {}.",
                    money(after),
                    money(expected)
                ),
            ));
        }
    }
    if let (Some(net), Some(vat), Some(gross)) =
        (totals.total_net, totals.vat_amount, totals.total_gross)
    {
        if round2(net + vat) != round2(gross) {
            out.push((
                "R8.gross",
                Severity::Error,
                "FD 3.3 11.4.2",
                ordinal.clone(),
                format!(
                    "{what}: Brutto {} ≠ Netto {} + USt {}.",
                    money(gross),
                    money(net),
                    money(vat)
                ),
            ));
        }
    }
    if let (Some(net), Some(percent), Some(vat)) =
        (totals.total_net, totals.vat_percent, totals.vat_amount)
    {
        let expected = round2(net * percent / Decimal::ONE_HUNDRED);
        if round2(vat) != expected {
            out.push((
                "R8.vat",
                Severity::Warning,
                "FD 3.3 11.4.2",
                ordinal,
                format!(
                    "{what}: USt {} ≠ {} % von {} = {}.",
                    money(vat),
                    plain(percent),
                    money(net),
                    money(expected)
                ),
            ));
        }
    }
}

fn image_count(text: &RichText) -> usize {
    text.blocks
        .iter()
        .filter(|block| matches!(block, TextBlock::Image { .. }))
        .count()
}

/// Dots or underscores as a gap to fill in ("....." or "_____").
fn has_gap_marks(text: &str) -> bool {
    let mut run = 0;
    let mut last = None;
    for ch in text.chars() {
        let (mark, width) = match ch {
            '.' => ('.', 1),
            '…' => ('.', 3),
            '_' => ('_', 1),
            _ => {
                run = 0;
                last = None;
                continue;
            }
        };
        run = if last == Some(mark) {
            run + width
        } else {
            width
        };
        last = Some(mark);
        if run >= 5 {
            return true;
        }
    }
    false
}

/// R10/R13 on the XML tree: complements in Kurz-/Langtext and Hinweistext.
fn raw_body(body: &Element, path: &mut Vec<String>, phase: &str, out: &mut Out) {
    let tender = phase == "83";
    for child in body.elements() {
        match child.name.as_str() {
            "BoQCtgy" => {
                path.push(child.attr("RNoPart").unwrap_or_default().trim().to_string());
                if let Some(inner) = child.child("BoQBody") {
                    raw_body(inner, path, phase, out);
                }
                path.pop();
            }
            "Itemlist" => raw_body(child, path, phase, out),
            "Item" | "MarkupItem" => {
                let mut pieces = path.clone();
                pieces.push(child.attr("RNoPart").unwrap_or_default().trim().to_string());
                if let Some(index) = child
                    .attr("RNoIndex")
                    .map(str::trim)
                    .filter(|i| !i.is_empty())
                {
                    pieces.push(index.to_string());
                }
                let oz = pieces.join(".");
                // X84 carries the Langtext only for the bidder's complements;
                // its schema has no ComplTSA/ComplTSB (Regeln für DA84).
                if let Some(description) = child.child("Description").filter(|_| phase != "84") {
                    raw_description(description, &oz, out);
                }
            }
            "Remark" if tender => {
                let bidder = descendants(child, "TextComplement")
                    .into_iter()
                    .any(|complement| {
                        complement
                            .attr("Kind")
                            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("bidder"))
                    });
                if bidder {
                    out.push(
                        "R13.remark_complement",
                        Severity::Warning,
                        "RIB e-Vergabe GAEB-Prüfer",
                        None,
                        format!(
                            "Hinweistext {} enthält eine Bietertextergänzung – Hinweistexte sind für den Bieter nicht bearbeitbar.",
                            if path.is_empty() { "im LV".to_string() } else { format!("in {}", path.join(".")) }
                        ),
                    );
                }
            }
            _ => {}
        }
    }
}

fn raw_description(description: &Element, oz: &str, out: &mut Out) {
    let Some(complete) = description.child("CompleteText") else {
        return;
    };
    let flag = |name: &str| {
        complete
            .child_text(name)
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("yes"))
    };
    let in_detail = complete
        .child("DetailTxt")
        .map(|detail| descendants(detail, "TextComplement"))
        .unwrap_or_default();
    let in_outline = complete
        .path(&["OutlineText", "OutlTxt"])
        .map(|outline| descendants(outline, "TextComplement"))
        .unwrap_or_default();
    if !in_detail.is_empty() && !flag("ComplTSA") && !flag("ComplTSB") {
        out.push(
            "R10.compl_ts",
            Severity::Error,
            "FD 3.3 11.2.3 Description 1",
            Some(oz.to_string()),
            format!("OZ {oz}: Langtext mit Textergänzungen ohne Kennzeichen ComplTSA/ComplTSB."),
        );
    }
    if !in_outline.is_empty() && !flag("ComplTSA") {
        out.push(
            "R10.compl_ts",
            Severity::Error,
            "FD 3.3 11.2.3 Description 2",
            Some(oz.to_string()),
            format!("OZ {oz}: Kurztext mit Textergänzungen ohne Kennzeichen ComplTSA."),
        );
    }
    if in_outline.iter().any(|complement| {
        complement
            .attr("Kind")
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("bidder"))
    }) {
        out.push(
            "R10.short_text_bidder",
            Severity::Error,
            "FD 3.3 4.6.16",
            Some(oz.to_string()),
            format!("OZ {oz}: Bietertextergänzung im Kurztext – dort sind nur Textergänzungen des Auftraggebers zulässig."),
        );
    }
}

fn descendants<'e>(element: &'e Element, name: &str) -> Vec<&'e Element> {
    let mut out = Vec::new();
    for child in element.elements() {
        if child.name == name {
            out.push(child);
        }
        out.extend(descendants(child, name));
    }
    out
}
