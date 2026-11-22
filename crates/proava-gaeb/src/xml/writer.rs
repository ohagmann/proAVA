//! GAEB DA XML 3.3 writer for the phases 81 to 86.

use std::collections::{HashMap, HashSet};

use rust_decimal::{Decimal, RoundingStrategy};

use super::calc;
use super::complement_place::{self, Segment};
use super::rich_text::{
    complement_font, write_blocks, write_complement, write_complement_in, write_inline_plain,
    write_runs,
};
use super::standard;
use super::GaebXmlError;
use crate::model::*;

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(' '),
            ch if (ch as u32) < 0x20 || ch == '\u{FFFE}' || ch == '\u{FFFF}' => {}
            ch => out.push(ch),
        }
    }
    out
}

fn truncate(text: &str, max: usize) -> String {
    let text = text.replace(['\n', '\r', '\t'], " ");
    text.chars().take(max).collect()
}

fn fixed(value: Decimal, places: u32) -> String {
    let mut value = value.round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero);
    value.rescale(places);
    value.to_string()
}

fn unit_price(value: Decimal, fraction_digits: Option<u8>) -> String {
    match fraction_digits {
        Some(places) => fixed(value, places.clamp(0, 3) as u32),
        None => fixed(value, value.scale().clamp(2, 3)),
    }
}

fn percent(value: Decimal) -> String {
    fixed(value, value.scale().clamp(0, 6))
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub(crate) fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn xml_time(time: &str) -> String {
    let time = time.trim();
    if time.len() == 5 && time.as_bytes()[2] == b':' {
        format!("{time}:00")
    } else {
        time.to_string()
    }
}

const CATEGORIES: [&str; 11] = [
    "OpenProc",
    "ClosedProc",
    "NegProc",
    "OpenCall",
    "SelectCall",
    "NegCont",
    "IntNATO",
    "SelectCallPostOpen",
    "NegProcOpen",
    "CompetDialog",
    "InnovationPartnership",
];

const UP_KINDS: [&str; 4] = ["Wages", "Materials", "Plant", "Miscellaneous"];

/// GAEB type (`LblUPComp` attribute `Type`) of the EP share at `index`: the
/// type from the file when GAEB knows it (including "Unknown", which other
/// programs write), else the type its usual German label stands for, else the
/// usual order Wages, Materials, Plant, Miscellaneous.
pub(crate) fn up_component_kind(label: &str, kind: &str, index: usize) -> &'static str {
    if let Some(known) = UP_KINDS
        .iter()
        .chain(&["Unknown"])
        .find(|known| **known == kind.trim())
    {
        return known;
    }
    match label.trim().to_lowercase().as_str() {
        "lohn" | "löhne" | "lohnanteil" => "Wages",
        "material" | "stoffe" | "stoff" | "materialanteil" => "Materials",
        "gerät" | "geräte" | "geraet" | "geraete" => "Plant",
        "sonstiges" | "sonstige" => "Miscellaneous",
        _ => UP_KINDS[index.min(3)],
    }
}

struct Out {
    text: String,
    depth: usize,
}

impl Out {
    fn line(&mut self, content: &str) {
        for _ in 0..self.depth {
            self.text.push_str("  ");
        }
        self.text.push_str(content);
        self.text.push('\n');
    }

    fn open(&mut self, tag: &str) {
        self.line(&format!("<{tag}>"));
        self.depth += 1;
    }

    fn open_with(&mut self, tag: &str, attrs: &[(&str, &str)]) {
        let attrs: String = attrs
            .iter()
            .map(|(key, value)| format!(" {key}=\"{}\"", escape(value)))
            .collect();
        self.line(&format!("<{tag}{attrs}>"));
        self.depth += 1;
    }

    fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.line(&format!("</{tag}>"));
    }

    fn leaf(&mut self, tag: &str, value: &str) {
        if value.is_empty() {
            self.line(&format!("<{tag}/>"));
        } else {
            self.line(&format!("<{tag}>{}</{tag}>", escape(value)));
        }
    }

    fn leaf_opt(&mut self, tag: &str, value: &str) {
        if !value.trim().is_empty() {
            self.leaf(tag, value.trim());
        }
    }

    fn yes(&mut self, tag: &str, flag: bool) {
        if flag {
            self.leaf(tag, "Yes");
        }
    }

    fn raw(&mut self, content: &str) {
        self.line(content);
    }
}

struct Writer<'a> {
    doc: &'a GaebDocument,
    phase: Phase,
    out: Out,
    item_ids: HashMap<String, String>,
    perf_ids: HashMap<String, String>,
    used_perf_ids: Vec<String>,
    /// Catalogs written in BoQInfo; assignments to others are left out.
    catalog_ids: Vec<String>,
    /// Items another item repeats (RefDescr "Ref").
    referenced: HashSet<String>,
    next_id: usize,
    images: bool,
}

/// Choices for writing DA XML.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// Write images of long texts (`<image>`, Base64). Off by default: GAEB
    /// exports of proAVA carry no images. Never in X84 (not allowed there).
    pub include_images: bool,
}

/// Whether the phase may carry images in long texts (DA XML 3.3).
pub(crate) fn images_allowed(phase: Phase, options: &WriteOptions) -> bool {
    options.include_images && phase != Phase::Dp84
}

/// Writes the document as GAEB DA XML 3.3 in the given phase.
pub fn write_gaeb_xml(
    doc: &GaebDocument,
    phase: Phase,
    options: &WriteOptions,
) -> Result<String, GaebXmlError> {
    if doc.award.boq.breakdown.is_empty() {
        return Err(GaebXmlError::Structure(
            "Die OZ-Maske (BoQBkdn) fehlt – ohne sie kann kein GAEB-LV geschrieben werden.".into(),
        ));
    }
    if !doc
        .award
        .boq
        .breakdown
        .iter()
        .any(|level| level.kind == BreakdownKind::Item)
    {
        return Err(GaebXmlError::Structure(
            "Die OZ-Maske hat keine Stufe für die Position.".into(),
        ));
    }
    let mut writer = Writer {
        doc,
        phase,
        out: Out {
            text: String::new(),
            depth: 0,
        },
        item_ids: HashMap::new(),
        perf_ids: HashMap::new(),
        used_perf_ids: Vec::new(),
        catalog_ids: Vec::new(),
        referenced: HashSet::new(),
        next_id: 1,
        images: images_allowed(phase, options),
    };
    writer.assign_ids(&doc.award.boq.body, &mut Vec::new());
    writer.document()?;
    Ok(writer.out.text)
}

impl Writer<'_> {
    fn id(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}{}", self.next_id);
        self.next_id += 1;
        id
    }

    fn assign_ids(&mut self, nodes: &[BoqNode], path: &mut Vec<String>) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    self.assign_ids(&category.body, path);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    let ordinal = calc::ordinal(path, item);
                    let id = self.id("I");
                    self.item_ids.entry(ordinal).or_insert(id);
                    if let Some(base) = &item.kind.repetition_of {
                        self.referenced.insert(base.clone());
                    }
                }
                BoqNode::PerfDescr(description) => {
                    let id = self.id("P");
                    self.perf_ids
                        .entry(description.number.clone())
                        .or_insert(id);
                }
                BoqNode::Remark(_) => {}
            }
        }
    }

    fn is(&self, number: u8) -> bool {
        self.phase.number() == number
    }

    fn prices(&self) -> bool {
        self.phase.has_prices()
    }

    fn document(&mut self) -> Result<(), GaebXmlError> {
        let doc = self.doc;
        self.out.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
        let namespace = format!(
            "http://www.gaeb.de/GAEB_DA_XML/DA{}/3.3",
            self.phase.number()
        );
        self.out.open_with("GAEB", &[("xmlns", &namespace)]);

        self.out.open("GAEBInfo");
        self.out.leaf("Version", "3.3");
        self.out.leaf("VersDate", "2021-05");
        let date = doc
            .date
            .clone()
            .filter(|date| date.len() == 10)
            .unwrap_or_else(today);
        self.out.leaf("Date", &date);
        self.out.leaf(
            "ProgSystem",
            &format!("proAVA {}", env!("CARGO_PKG_VERSION")),
        );
        self.out.leaf("ProgName", "proAVA");
        self.out.close("GAEBInfo");

        self.project();
        self.award()?;
        // Schlussbemerkungen after the Award (GAEB/AddText; DA 84 has none).
        if !self.is(84) {
            for text in &doc.award.closing_texts {
                self.add_text(text);
            }
        }
        self.out.close("GAEB");
        Ok(())
    }

    fn project(&mut self) {
        let project = &self.doc.project;
        self.out.open("PrjInfo");
        self.out.leaf("NamePrj", &truncate(&project.name, 60));
        if !project.label.is_empty() {
            self.out.leaf("LblPrj", &truncate(&project.label, 100));
        }
        if !self.is(84) {
            if !project.currency.is_empty() {
                self.out.leaf("Cur", &truncate(&project.currency, 4));
                self.out
                    .leaf_opt("CurLbl", &truncate(&project.currency_label, 60));
            }
            if let Some(digits) = project
                .up_fraction_digits
                .filter(|digits| (2..=3).contains(digits))
            {
                self.out.leaf("UPFracDig", &digits.to_string());
            }
        }
        self.out.close("PrjInfo");
    }

    fn award(&mut self) -> Result<(), GaebXmlError> {
        let award = &self.doc.award;
        let project = &self.doc.project;
        self.out.open("Award");
        self.out.leaf("DP", &self.phase.number().to_string());

        self.out.open("AwardInfo");
        let currency = truncate(&project.currency, 4);
        let currency_label = truncate(&project.currency_label, 60);
        if self.is(84) || self.is(86) {
            // DA84/DA86 AwardInfo: no Cat and no submission data.
            if !currency.is_empty() {
                self.out.leaf("Cur", &currency);
                self.out.leaf_opt("CurLbl", &currency_label);
            }
            if let Some(date) = &award.bid_date {
                self.out.leaf("BidDate", date);
            }
            if self.is(86) {
                if let Some(date) = &award.submission.construction_start {
                    self.out.leaf("CnstStart", date);
                }
                if let Some(date) = &award.submission.construction_end {
                    self.out.leaf("CnstEnd", date);
                }
            }
        } else {
            if CATEGORIES.contains(&award.category.as_str()) {
                self.out.leaf("Cat", &award.category);
            }
            if !currency.is_empty() {
                self.out.leaf("Cur", &currency);
                self.out.leaf_opt("CurLbl", &currency_label);
            }
            if !self.is(83) {
                if let Some(date) = &award.bid_date {
                    self.out.leaf("BidDate", date);
                }
            }
            let submission = &award.submission;
            if let Some(date) = &submission.open_date {
                self.out.leaf("OpenDate", date);
                if let Some(time) = &submission.open_time {
                    self.out.leaf("OpenTime", &xml_time(time));
                }
            }
            if let Some(date) = &submission.award_deadline {
                self.out.leaf("EvalEnd", date);
            }
            self.out
                .leaf_opt("SubmLoc", &truncate(&submission.location, 60));
            if let Some(date) = &submission.construction_start {
                self.out.leaf("CnstStart", date);
            }
            if let Some(date) = &submission.construction_end {
                self.out.leaf("CnstEnd", date);
            }
        }
        self.out.close("AwardInfo");

        // DA86 requires OWN and CTR (with address), DA84 CTR.
        if self.is(86)
            || (!self.is(84)
                && (award.owner.is_some()
                    || !award.award_no.is_empty()
                    || !award.owner_dp_no.is_empty()
                    || !award.owner_account.is_empty()
                    || !award.owner_country.kind.is_empty()))
        {
            self.out.open("OWN");
            match &award.owner {
                Some(owner) => self.address(owner),
                None if self.is(86) => self.address(&Address::default()),
                None => {}
            }
            self.out.leaf_opt("DPNo", &truncate(&award.owner_dp_no, 60));
            self.out.leaf_opt("AwardNo", &truncate(&award.award_no, 60));
            self.out
                .leaf_opt("AcctRecNo", &truncate(&award.owner_account, 20));
            self.country(&award.owner_country);
            self.out.close("OWN");
        }
        // Requester, CnstSite, NotifSite: not in bids (84/85).
        let places = !self.is(84) && !self.is(85);
        if places {
            for requester in &award.requesters {
                self.place("Requester", "Request", requester);
            }
        }
        if self.is(84) || self.is(86) {
            self.out.open("CTR");
            self.address(&award.bidder.clone().unwrap_or_default());
            self.out
                .leaf_opt("DPNo", &truncate(&award.bidder_dp_no, 60));
            self.out.close("CTR");
        } else if !self.is(83) {
            if let Some(bidder) = &award.bidder {
                self.out.open("CTR");
                self.address(bidder);
                self.out
                    .leaf_opt("DPNo", &truncate(&award.bidder_dp_no, 60));
                self.out.close("CTR");
            }
        }
        if places {
            for site in &award.sites {
                self.place("CnstSite", "CnstSite", &site.site);
                if let Some(notification) = &site.notification {
                    self.place("NotifSite", "NotifSite", notification);
                }
            }
        }
        if !self.is(84) {
            for text in &award.add_texts {
                self.add_text(text);
            }
        }
        self.boq()?;
        self.out.close("Award");
        Ok(())
    }

    fn add_text(&mut self, text: &AddText) {
        self.out.open("AddText");
        if !text.outline.is_empty() {
            let mut inline = String::new();
            write_inline_plain(&text.outline, &mut inline);
            self.out
                .raw(&format!("<OutlineAddText>{inline}</OutlineAddText>"));
        }
        let mut detail = String::new();
        write_blocks(&text.detail, self.images, &mut detail);
        if detail.is_empty() {
            self.out.raw("<DetailAddText/>");
        } else {
            self.out
                .raw(&format!("<DetailAddText>{detail}</DetailAddText>"));
        }
        self.out.close("AddText");
    }

    /// Requester, CnstSite or NotifSite (`prefix` of IDNo/Name).
    fn place(&mut self, element: &str, prefix: &str, place: &Place) {
        self.out.open(element);
        if let Some(address) = &place.address {
            self.address(address);
        }
        self.out
            .leaf_opt(&format!("{prefix}IDNo"), &truncate(&place.id_no, 20));
        self.out
            .leaf_opt(&format!("{prefix}Name"), &truncate(&place.name, 60));
        self.out.close(element);
    }

    /// CntryType („EEA“/„Other“) and CntryName (only with a type).
    fn country(&mut self, country: &Country) {
        let kind = country.kind.trim();
        if kind == "EEA" || kind == "Other" {
            self.out.leaf("CntryType", kind);
            self.out.leaf_opt("CntryName", &truncate(&country.name, 3));
        }
    }

    fn address(&mut self, address: &Address) {
        self.out.open("Address");
        self.out.leaf("Name1", &truncate(&address.name1, 40));
        self.out.leaf_opt("Name2", &truncate(&address.name2, 40));
        self.out.leaf_opt("Name3", &truncate(&address.name3, 40));
        self.out.leaf_opt("Name4", &truncate(&address.name4, 40));
        self.out.leaf("Street", &truncate(&address.street, 40));
        self.out.leaf("PCode", &truncate(&address.postal_code, 20));
        self.out.leaf("City", &truncate(&address.city, 40));
        self.out
            .leaf_opt("Country", &truncate(&address.country, 40));
        self.out.leaf_opt("ILN", &truncate(&address.iln, 20));
        self.out
            .leaf_opt("Contact", &truncate(&address.contact, 40));
        self.out.leaf_opt("Phone", &truncate(&address.phone, 20));
        self.out.leaf_opt("Fax", &truncate(&address.fax, 20));
        self.out.leaf_opt("Email", &truncate(&address.email, 256));
        self.out.leaf_opt("VATID", &truncate(&address.vat_id, 80));
        self.out.close("Address");
    }

    fn boq(&mut self) -> Result<(), GaebXmlError> {
        let boq = &self.doc.award.boq;
        let id = self.id("B");
        self.out.open_with("BoQ", &[("ID", &id)]);
        self.out.open("BoQInfo");
        self.out.leaf("Name", &truncate(&boq.name, 20));
        if !self.is(84) {
            self.out.leaf("LblBoQ", &truncate(&boq.label, 60));
            if let Some(tag) = &boq.change_order {
                self.change_order_tag(tag);
            }
            if let Some(date) = &boq.date {
                self.out.leaf("Date", date);
            }
            self.out.leaf("OutlCompl", "AllTxt");
        }
        for level in &boq.breakdown {
            self.out.open("BoQBkdn");
            let kind = match level.kind {
                BreakdownKind::Lot => "Lot",
                BreakdownKind::Level => "BoQLevel",
                BreakdownKind::Item => "Item",
                BreakdownKind::Index => "Index",
            };
            self.out.leaf("Type", kind);
            if level.kind == BreakdownKind::Level {
                self.out.leaf_opt("LblBoQBkdn", &truncate(&level.label, 40));
            }
            let length = if level.kind == BreakdownKind::Index {
                1
            } else {
                level.length.clamp(1, 14)
            };
            self.out.leaf("Length", &length.to_string());
            self.out
                .leaf("Num", if level.numeric { "Yes" } else { "No" });
            self.out.close("BoQBkdn");
        }
        if !self.is(84) && !boq.up_component_labels.is_empty() {
            let count = boq.up_component_labels.len().clamp(2, 6);
            self.out.leaf("NoUPComps", &count.to_string());
            for index in 0..count {
                let label = boq
                    .up_component_labels
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "Sonstiges".into());
                let kind = up_component_kind(
                    &label,
                    boq.up_component_kinds
                        .get(index)
                        .map(String::as_str)
                        .unwrap_or_default(),
                    index,
                );
                self.out.raw(&format!(
                    "<LblUPComp{n} Type=\"{kind}\">{}</LblUPComp{n}>",
                    escape(&truncate(&label, 20)),
                    n = index + 1
                ));
            }
        }
        if !self.is(84) {
            self.out.leaf_opt("LblTime", &truncate(&boq.time_label, 20));
        }
        if self.writes_totals() {
            let mut totals = boq.totals.clone().unwrap_or_default();
            if totals.total.is_none() {
                totals.total = Some(calc::nodes_total(&boq.body, &mut Vec::new()));
            }
            self.totals(&totals);
        }
        if !self.is(84) {
            for catalog in self.used_catalogs(boq) {
                self.out.raw(&format!(
                    "<Ctlg><CtlgID>{}</CtlgID><CtlgType>{}</CtlgType><CtlgName>{}</CtlgName></Ctlg>",
                    escape(&truncate(&catalog.id, 100)),
                    escape(&catalog.kind),
                    escape(&truncate(&catalog.name, 80)),
                ));
            }
        }
        self.out.close("BoQInfo");
        self.body(&boq.body, &mut Vec::new())?;
        self.out.close("BoQ");
        Ok(())
    }

    /// CtlgAssign elements for assignments to catalogs defined in BoQInfo.
    fn catalog_assigns(&mut self, assigns: &[CatalogAssign]) {
        for assign in assigns {
            if !self.catalog_ids.contains(&assign.catalog) || assign.code.trim().is_empty() {
                continue;
            }
            self.out.raw(&format!(
                "<CtlgAssign><CtlgID>{}</CtlgID><CtlgCode>{}</CtlgCode></CtlgAssign>",
                escape(&truncate(&assign.catalog, 100)),
                escape(&truncate(assign.code.trim(), 100)),
            ));
        }
    }

    /// Catalogs of the BoQ that have an id (written in BoQInfo, remembered
    /// so that only assignments to them are written).
    fn used_catalogs(&mut self, boq: &Boq) -> Vec<Catalog> {
        let catalogs: Vec<Catalog> = boq
            .catalogs
            .iter()
            .filter(|catalog| !catalog.id.trim().is_empty() && !catalog.kind.trim().is_empty())
            .cloned()
            .collect();
        self.catalog_ids = catalogs.iter().map(|catalog| catalog.id.clone()).collect();
        catalogs
    }

    fn writes_totals(&self) -> bool {
        self.prices() && !self.is(83)
    }

    fn totals(&mut self, totals: &Totals) {
        self.out.open("Totals");
        self.out
            .leaf("Total", &fixed(totals.total.unwrap_or_default(), 2));
        if let Some(after) = totals.total_after_discount {
            if let Some(pcnt) = totals.discount_percent {
                self.out.leaf("DiscountPcnt", &percent(pcnt));
                self.out.leaf("TotAfterDisc", &fixed(after, 2));
            } else if let Some(amount) = totals.discount_amount {
                self.out.leaf("DiscountAmt", &fixed(amount, 2));
                self.out.leaf("TotAfterDisc", &fixed(after, 2));
            }
        } else if let Some(lump) = totals.lump_sum {
            self.out.leaf("TotalLSUM", &fixed(lump, 2));
        }
        if let Some(vat) = totals.vat_percent {
            self.out.leaf("VAT", &fixed(vat, 2));
        }
        if let Some(net) = totals.total_net {
            self.out.leaf("TotalNet", &fixed(net, 2));
        }
        if let Some(amount) = totals.vat_amount {
            self.out.leaf("VATAmount", &fixed(amount, 2));
        }
        if let Some(gross) = totals.total_gross {
            self.out.leaf("TotalGross", &fixed(gross, 2));
        }
        self.out.close("Totals");
    }

    fn body(&mut self, nodes: &[BoqNode], path: &mut Vec<String>) -> Result<(), GaebXmlError> {
        let has_categories = nodes
            .iter()
            .any(|node| matches!(node, BoqNode::Category(_)));
        if nodes.is_empty() {
            self.out.raw("<BoQBody/>");
            return Ok(());
        }
        self.out.open("BoQBody");
        if has_categories {
            for node in nodes {
                match node {
                    BoqNode::Category(category) => self.category(category, path)?,
                    BoqNode::Remark(remark) => self.remark(remark),
                    BoqNode::PerfDescr(description) => self.performance(description),
                    BoqNode::Item(item) => {
                        return Err(GaebXmlError::Structure(format!(
                            "Position {} steht neben Titeln in derselben Stufe – GAEB erlaubt in \
                             einer Stufe nur Titel oder nur Positionen.",
                            calc::ordinal(path, item)
                        )))
                    }
                }
            }
        } else {
            self.out.open("Itemlist");
            for node in nodes {
                match node {
                    BoqNode::Item(item) => self.item(item, path),
                    BoqNode::Remark(remark) => self.remark(remark),
                    BoqNode::PerfDescr(description) => self.performance(description),
                    BoqNode::Category(_) => unreachable!(),
                }
            }
            self.out.close("Itemlist");
        }
        self.out.close("BoQBody");
        Ok(())
    }

    fn category(
        &mut self,
        category: &Category,
        path: &mut Vec<String>,
    ) -> Result<(), GaebXmlError> {
        let id = self.id("C");
        self.out
            .open_with("BoQCtgy", &[("ID", &id), ("RNoPart", &category.rno_part)]);
        if !self.is(84) {
            let mut label = String::new();
            write_inline_plain(&category.label, &mut label);
            if label.is_empty() {
                self.out.raw("<LblTx/>");
            } else {
                self.out.raw(&format!("<LblTx><p>{label}</p></LblTx>"));
            }
            if let Some(alternative) = &category.alternative {
                let group: u32 = alternative.group.trim().parse().unwrap_or(0);
                let serial: u32 = alternative.serial.trim().parse().unwrap_or(0);
                self.out.leaf("ALNBGroupNo", &group.min(999).to_string());
                self.out.leaf("ALNBSerNo", &serial.min(9).to_string());
            }
            self.catalog_assigns(&category.catalog_assigns);
        }
        path.push(category.rno_part.clone());
        self.body(&category.body, path)?;
        if self.writes_totals() {
            let mut totals = category.totals.clone().unwrap_or_default();
            if totals.total.is_none() {
                totals.total = Some(calc::nodes_total(&category.body, path));
            }
            self.totals(&totals);
        }
        path.pop();
        self.out.close("BoQCtgy");
        Ok(())
    }

    fn remark(&mut self, remark: &Remark) {
        if self.is(84) {
            return;
        }
        let id = self.id("R");
        self.out.open_with("Remark", &[("ID", &id)]);
        let standard = standard::write(remark.stlb.as_ref(), remark.standard_number.as_ref());
        self.description(
            &remark.short_text,
            &remark.text,
            &remark.complements,
            true,
            &standard,
        );
        self.out.close("Remark");
    }

    fn performance(&mut self, description: &PerfDescr) {
        if self.is(84) {
            return;
        }
        let id = match self.perf_ids.get(&description.number) {
            Some(id) if !self.used_perf_ids.contains(id) => id.clone(),
            _ => self.id("P"),
        };
        self.used_perf_ids.push(id.clone());
        self.out.open_with("PerfDescr", &[("ID", &id)]);
        let number: u32 = description.number.trim().parse().unwrap_or(0);
        self.out.leaf("PerfNo", &number.min(9999).to_string());
        self.out.leaf("PerfLbl", &truncate(&description.label, 55));
        let standard = standard::write(
            description.stlb.as_ref(),
            description.standard_number.as_ref(),
        );
        self.description(
            &description.short_text,
            &description.long_text,
            &description.complements,
            true,
            &standard,
        );
        self.out.close("PerfDescr");
    }

    /// Description with Kurztext and Langtext. `required` writes an empty
    /// OutlineText when there is no text at all. `standard` is the STLBBau or
    /// StLNo element from [`standard::write`], empty for free texts.
    fn description(
        &mut self,
        short: &str,
        long: &RichText,
        complements: &[TextComplement],
        required: bool,
        standard: &str,
    ) {
        if long.blocks.is_empty() && complements.is_empty() {
            if short.is_empty() && !required && standard.is_empty() {
                return;
            }
            let mut inline = String::new();
            write_inline_plain(short, &mut inline);
            self.out.open("Description");
            if !standard.is_empty() {
                self.out.raw(standard);
            }
            self.out.raw(&format!(
                "<OutlineText><OutlTxt><TextOutlTxt>{inline}</TextOutlTxt></OutlTxt></OutlineText>"
            ));
            self.out.close("Description");
            return;
        }
        self.out.open("Description");
        if !standard.is_empty() {
            self.out.raw(standard);
        }
        self.out.open("CompleteText");
        if complements
            .iter()
            .any(|complement| complement.kind == "Owner")
        {
            self.out.leaf("ComplTSA", "Yes");
        }
        if complements
            .iter()
            .any(|complement| complement.kind != "Owner")
        {
            self.out.leaf("ComplTSB", "Yes");
        }
        let mut detail = String::new();
        let mut text = String::new();
        let flush = |text: &mut String, detail: &mut String| {
            if !text.is_empty() {
                detail.push_str("<Text>");
                detail.push_str(text);
                detail.push_str("</Text>");
                text.clear();
            }
        };
        // Caption and tail of a complement take the font of the text before
        // it (AVA programs write them so; texts without font stay as they are).
        let mut font = String::new();
        match complement_place::segments(long, complements) {
            // Complements inside the text: at their anchors.
            Some(segments) => {
                for segment in segments {
                    match segment {
                        Segment::Block(index) => {
                            let single = RichText {
                                blocks: vec![long.blocks[index].clone()],
                            };
                            write_blocks(&single, self.images, &mut text);
                            if let Some(css) = complement_font(&long.blocks[index]) {
                                font = css;
                            }
                        }
                        Segment::Bare(runs) => {
                            write_runs(&runs, &mut text);
                            if let Some(run) =
                                runs.iter().rev().find(|run| !run.text.trim().is_empty())
                            {
                                font = super::rich_text::font_css(run);
                            }
                        }
                        Segment::Break => text.push_str("<br/>"),
                        Segment::Start(align) => {
                            text.push_str(&super::rich_text::paragraph_start_span(align));
                        }
                        Segment::Complement(index) => {
                            flush(&mut text, &mut detail);
                            write_complement_in(&complements[index], false, &font, &mut detail);
                        }
                    }
                }
                flush(&mut text, &mut detail);
            }
            // No place inside the text: the complements follow it.
            None => {
                write_blocks(long, self.images, &mut text);
                flush(&mut text, &mut detail);
                if let Some(css) = long.blocks.iter().rev().find_map(complement_font) {
                    font = css;
                }
                for complement in complements {
                    write_complement_in(complement, false, &font, &mut detail);
                }
            }
        }
        if detail.is_empty() {
            detail.push_str("<Text/>");
        }
        self.out.raw(&format!("<DetailTxt>{detail}</DetailTxt>"));
        if !short.is_empty() {
            let mut inline = String::new();
            write_inline_plain(short, &mut inline);
            self.out.raw(&format!(
                "<OutlineText><OutlTxt><TextOutlTxt>{inline}</TextOutlTxt></OutlTxt></OutlineText>"
            ));
        }
        self.out.close("CompleteText");
        self.out.close("Description");
    }

    fn reference(&mut self, item: &Item, path: &[String]) {
        let repeated = item
            .kind
            .repetition_of
            .as_ref()
            .and_then(|ordinal| self.item_ids.get(ordinal))
            .cloned();
        if repeated.is_some() {
            self.out.leaf("RefDescr", "Rep");
        } else if self.referenced.contains(&calc::ordinal(path, item)) {
            self.out.leaf("RefDescr", "Ref");
        }
        if let Some(id) = repeated {
            self.out
                .raw(&format!("<RefRNo IDRef=\"{}\"/>", escape(&id)));
        } else if let Some(id) = item
            .kind
            .reference_description
            .as_ref()
            .and_then(|number| self.perf_ids.get(number))
        {
            self.out
                .raw(&format!("<RefPerfNo IDRef=\"{}\"/>", escape(id)));
        }
    }

    fn alternative(&mut self, item: &Item) {
        if let Some(alternative) = &item.kind.alternative {
            let group: u32 = alternative.group.trim().parse().unwrap_or(0);
            let serial: u32 = alternative.serial.trim().parse().unwrap_or(0);
            self.out.leaf("ALNGroupNo", &group.min(999).to_string());
            self.out.leaf("ALNSerNo", &serial.min(9).to_string());
        }
        if let Some(provisional) = item.kind.provisional {
            self.out.leaf(
                "Provis",
                match provisional {
                    Provisional::WithoutTotal => "WithoutTotal",
                    Provisional::WithTotal => "WithTotal",
                },
            );
        }
    }

    fn item(&mut self, item: &Item, path: &[String]) {
        let ordinal = calc::ordinal(path, item);
        let id = self
            .item_ids
            .get(&ordinal)
            .cloned()
            .unwrap_or_else(|| self.id("I"));
        let mut attrs = vec![("ID", id.as_str()), ("RNoPart", item.rno_part.as_str())];
        if !item.rno_index.is_empty() {
            attrs.push(("RNoIndex", item.rno_index.as_str()));
        }
        if calc::is_surcharge(item) {
            self.out.open_with("MarkupItem", &attrs);
            self.markup_item(item, path);
            self.out.close("MarkupItem");
            return;
        }
        self.out.open_with("Item", &attrs);
        if self.is(84) {
            self.bid_item(item);
            self.out.close("Item");
            return;
        }
        let tender = self.is(83);
        self.alternative(item);
        self.out.yes("LumpSumItem", item.kind.lump_sum);
        if !tender {
            self.out.yes("NotAppl", item.not_applicable);
        }
        self.out.yes("HourIt", item.kind.hourly);
        self.out.yes("KeyIt", item.key_item);
        self.out.yes("UPBkdn", item.up_breakdown);
        self.out.yes("MarkupIt", item.kind.markup);
        if let Some(tag) = &item.change_order {
            self.change_order_tag(tag);
        }
        self.reference(item, path);
        if item.qty_to_be_determined {
            self.out.leaf("QtyTBD", "Yes");
            if !tender {
                if let Some(qty) = item.qty {
                    self.out.leaf("Qty", &fixed(qty, 3));
                }
            }
        } else if let Some(qty) = item.qty {
            self.out.leaf("Qty", &fixed(qty, 3));
        }
        if tender || !item.unit.is_empty() {
            self.out.leaf("QU", &truncate(&item.unit, 4));
        }
        if !self.is(84) {
            self.catalog_assigns(&item.catalog_assigns);
        }
        if self.prices() {
            self.prices_of(item);
            if let Some(hours) = item.time_quantity {
                self.out.leaf("TimeQu", &fixed(hours, 2));
            }
        }
        let standard = standard::write(item.stlb.as_ref(), item.standard_number.as_ref());
        self.description(
            &item.short_text,
            &item.long_text,
            &item.complements,
            false,
            &standard,
        );
        self.sub_descriptions(item);
        self.out.close("Item");
    }

    fn prices_of(&mut self, item: &Item) {
        let digits = self.doc.project.up_fraction_digits;
        if let Some(price) = item.unit_price {
            self.out.leaf("UP", &unit_price(price, digits));
            for (index, component) in item.up_components.iter().enumerate().take(6) {
                if let Some(component) = component {
                    self.out.leaf(
                        &format!("UPComp{}", index + 1),
                        &unit_price(*component, digits),
                    );
                }
            }
            if let Some(discount) = item.discount_percent {
                self.out.leaf("DiscountPcnt", &percent(discount));
            }
        }
        let total = item
            .total
            .or_else(|| calc::item_total(item).filter(|_| calc::shows_total(item)));
        if let Some(total) = total {
            self.out.leaf("IT", &fixed(total, 2));
        }
    }

    fn bid_item(&mut self, item: &Item) {
        self.out.yes("NotOffered", item.not_offered);
        if item.qty_to_be_determined {
            if let Some(qty) = item.qty {
                self.out.leaf("Qty", &fixed(qty, 3));
            }
        }
        self.prices_of(item);
        if let Some(hours) = item.time_quantity {
            self.out.leaf("TimeQu", &fixed(hours, 2));
        }
        let bidder: Vec<&TextComplement> = item
            .complements
            .iter()
            .filter(|complement| complement.kind != "Owner")
            .collect();
        if !bidder.is_empty() {
            let mut detail = String::new();
            for complement in bidder {
                write_complement(complement, true, &mut detail);
            }
            self.out.raw(&format!(
                "<Description><CompleteText><DetailTxt>{detail}</DetailTxt></CompleteText></Description>"
            ));
        }
    }

    /// CONo and COStatus (not in phase 84, whose schema has no Nachtrag).
    fn change_order_tag(&mut self, tag: &ChangeOrderTag) {
        if self.is(84) {
            return;
        }
        self.out.leaf("CONo", &tag.number.min(999).to_string());
        let status = if tag.status.trim().is_empty() {
            "Filed"
        } else {
            tag.status.trim()
        };
        self.out.leaf("COStatus", status);
    }

    fn markup_item(&mut self, item: &Item, path: &[String]) {
        let default = Surcharge::default();
        let surcharge = item.surcharge.as_ref().unwrap_or(&default);
        if !self.is(84) {
            self.alternative(item);
            self.out.yes("HourIt", item.kind.hourly);
            self.out.yes("KeyIt", item.key_item);
            self.reference(item, path);
            let kind = match surcharge.markup_type.as_str() {
                kind @ ("IdentAsMark" | "AllInCat" | "ListInSubQty") => kind,
                // GAEB 90 surcharges apply to the items marked with "Z".
                _ => "IdentAsMark",
            };
            self.out.leaf("MarkupType", kind);
            if kind == "ListInSubQty" {
                for reference in &surcharge.items {
                    if let Some(id) = self.item_ids.get(&reference.ordinal).cloned() {
                        self.out.open("MarkupSubQty");
                        self.out
                            .raw(&format!("<RefItem IDRef=\"{}\"/>", escape(&id)));
                        if let Some(qty) = reference.sub_qty {
                            self.out.leaf("SubQty", &fixed(qty, 3));
                        }
                        self.out.close("MarkupSubQty");
                    }
                }
            }
        }
        if self.prices() {
            // DA86 requires both.
            let required = self.is(86);
            if let Some(base) = item.markup_base.or(required.then_some(Decimal::ZERO)) {
                self.out.leaf("ITMarkup", &fixed(base, 2));
            }
            if let Some(pcnt) = item.markup_percent.or(required.then_some(Decimal::ZERO)) {
                self.out.leaf("Markup", &percent(pcnt));
            }
            if let Some(discount) = item.discount_percent {
                self.out.leaf("DiscountPcnt", &percent(discount));
            }
            if let Some(total) = item.total.or_else(|| calc::item_total(item)) {
                self.out.leaf("IT", &fixed(total, 2));
            }
        }
        if !self.is(84) {
            let standard = standard::write(item.stlb.as_ref(), item.standard_number.as_ref());
            self.description(
                &item.short_text,
                &item.long_text,
                &item.complements,
                true,
                &standard,
            );
            // In MarkupItem the catalogue assignments follow the description.
            self.catalog_assigns(&item.catalog_assigns);
        }
    }

    fn sub_descriptions(&mut self, item: &Item) {
        if item.sub_descriptions.is_empty() {
            return;
        }
        self.out.leaf("SumDescr", "Yes");
        for (index, sub) in item.sub_descriptions.iter().enumerate() {
            self.out.open("SubDescr");
            // Valid numbers stay as written ("01"), xs:positiveInteger allows it.
            let number = match sub.number.trim() {
                number if number.parse::<u32>().is_ok_and(|n| (1..=99).contains(&n)) => {
                    number.to_string()
                }
                _ => (index + 1).to_string(),
            };
            self.out.leaf("SubDNo", &number);
            let standard = standard::write(sub.stlb.as_ref(), sub.standard_number.as_ref());
            self.description(&sub.short_text, &sub.long_text, &[], true, &standard);
            if let Some(qty) = sub.qty {
                self.out.leaf("Qty", &fixed(qty, 3));
                self.out.leaf_opt("QU", &truncate(&sub.unit, 4));
            }
            self.out.close("SubDescr");
        }
    }
}
