//! GAEB DA XML 3.3 for the Abrechnung: X89/X89B (Rechnung) and X31
//! (Mengenermittlung). Reads only what the check needs: invoice header,
//! contractor, per item the ordinal, billed quantity, unit, price, total,
//! check note, change order number and the quantity takeoff (total and the
//! REB 23.003 rows as text).

use std::str::FromStr;

use rust_decimal::Decimal;

use super::dom::{self, Element};
use super::reader::{address, description};
use super::{GaebXmlError, XmlReadReport};
use crate::model::Address;

/// What kind of billing file it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingPhase {
    /// X89 – Rechnung.
    Invoice,
    /// X89B – Rechnung (reduced, „Rechnungsbeilage“).
    InvoiceB,
    /// X31 – Mengenermittlung.
    Takeoff,
}

impl BillingPhase {
    pub fn label(self) -> &'static str {
        match self {
            BillingPhase::Invoice => "X89",
            BillingPhase::InvoiceB => "X89B",
            BillingPhase::Takeoff => "X31",
        }
    }
}

/// InvoiceHeader of an X89.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GaebInvoiceHeader {
    pub number: String,
    /// As written in the file (JJJJ-MM-TT).
    pub date: String,
    /// InvoiceType: „deduction“ (Abschlag), „final account“, „part final
    /// account“, „advance payment“, „single invoice“ …
    pub kind: String,
    /// SettlementType: „accumulated“ (kumuliert) or „periodic“ (Zuwachs).
    pub settlement: String,
    pub sequential_no: Option<u32>,
    pub period_start: String,
    pub period_end: String,
    pub credit_note: bool,
}

/// One REB 23.003 row of a takeoff.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GaebTakeoffRow {
    /// QTakeoff Row, columns 12 to 80 (the REB part), right-trimmed.
    pub row: String,
    /// BVBS:Explanation (long explanation), if any.
    pub explanation: String,
}

/// QtyDeterm of an item: the total and its rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GaebTakeoff {
    pub qty: Option<Decimal>,
    pub rows: Vec<GaebTakeoffRow>,
}

/// One item of a billing file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GaebBillItem {
    /// RNoPart of the categories and the item.
    pub pieces: Vec<String>,
    pub rno_index: String,
    pub short_text: String,
    /// BillQty (X89) – the quantity invoiced.
    pub bill_qty: Option<Decimal>,
    pub unit: String,
    pub unit_price: Option<Decimal>,
    pub total: Option<Decimal>,
    pub lump_sum: bool,
    /// CONo – change order number.
    pub change_order: String,
    /// CheckNote/Annotation.
    pub check_note: String,
    pub takeoff: Option<GaebTakeoff>,
}

/// A billing file read.
#[derive(Debug, Clone, PartialEq)]
pub struct GaebBilling {
    pub phase: BillingPhase,
    pub project_name: String,
    pub contractor: Option<Address>,
    /// Only X89/X89B.
    pub header: Option<GaebInvoiceHeader>,
    /// BoQInfo/Totals/Total – the net total stated (X89).
    pub total: Option<Decimal>,
    pub items: Vec<GaebBillItem>,
}

/// Reads an X89, X89B or X31 file.
pub fn read_billing(bytes: &[u8]) -> Result<(GaebBilling, XmlReadReport), GaebXmlError> {
    let text = dom::decode(bytes);
    let root = dom::parse(&text)?;
    if root.name != "GAEB" {
        return Err(GaebXmlError::NotGaeb(format!(
            "das Wurzelelement heißt <{}> statt <GAEB>",
            root.name
        )));
    }
    let namespace = root.attr("xmlns").unwrap_or_default().to_string();
    let version = root
        .child("GAEBInfo")
        .and_then(|info| info.child_text("Version"))
        .unwrap_or_default();
    let (container, phase) = if let Some(invoice) = root.child("Invoice") {
        let dp = invoice.child_text("DP").unwrap_or_default();
        let phase = if dp.trim().eq_ignore_ascii_case("89B") || namespace.contains("89B") {
            BillingPhase::InvoiceB
        } else {
            BillingPhase::Invoice
        };
        (invoice, phase)
    } else if let Some(takeoff) = root.child("QtyDeterm") {
        (takeoff, BillingPhase::Takeoff)
    } else if root.child("Award").is_some() {
        return Err(GaebXmlError::NotGaeb(
            "die Datei ist ein Leistungsverzeichnis (X81–X86), keine Rechnung (X89) oder Mengenermittlung (X31)"
                .into(),
        ));
    } else {
        return Err(GaebXmlError::NotGaeb(
            "die Datei enthält weder eine Rechnung (Invoice) noch eine Mengenermittlung (QtyDeterm)".into(),
        ));
    };
    let mut warnings = Vec::new();
    let project_name = root
        .child("PrjInfo")
        .or_else(|| container.child("PrjInfo"))
        .and_then(|info| info.child_text("NamePrj"))
        .unwrap_or_default();
    let header = container.child("InvoiceHeader").map(|header| {
        let text = |name: &str| {
            header
                .child_text(name)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        GaebInvoiceHeader {
            number: text("InvoiceNo"),
            date: text("InvoiceDate"),
            kind: text("InvoiceType"),
            settlement: text("SettlementType"),
            sequential_no: header
                .child_text("SequentialNo")
                .and_then(|n| n.trim().parse().ok()),
            period_start: text("ServiceProvisionStartDate"),
            period_end: text("ServiceProvisionEndDate"),
            credit_note: header
                .child_text("CreditNote")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("yes")),
        }
    });
    let boq = container.child("BoQ");
    let total = boq
        .and_then(|boq| boq.path(&["BoQInfo", "Totals", "Total"]))
        .and_then(|total| number(&total.text(), "Total", &mut warnings));
    let mut items = Vec::new();
    match boq.and_then(|boq| boq.child("BoQBody")) {
        Some(body) => walk(body, &mut Vec::new(), &mut items, &mut warnings),
        None => warnings.push("Die Datei enthält keine Positionen (BoQBody).".into()),
    }
    Ok((
        GaebBilling {
            phase,
            project_name,
            contractor: container.path(&["CTR", "Address"]).map(address),
            header,
            total,
            items,
        },
        XmlReadReport {
            version,
            namespace,
            warnings,
        },
    ))
}

fn number(raw: &str, name: &str, warnings: &mut Vec<String>) -> Option<Decimal> {
    let cleaned = raw.trim().replace(',', ".");
    if cleaned.is_empty() {
        return None;
    }
    match Decimal::from_str(&cleaned).or_else(|_| Decimal::from_scientific(&cleaned)) {
        Ok(value) => Some(value),
        Err(_) => {
            warnings.push(format!(
                "Wert „{raw}“ in <{name}> ist keine Zahl und wurde übergangen."
            ));
            None
        }
    }
}

fn child_number(element: &Element, name: &str, warnings: &mut Vec<String>) -> Option<Decimal> {
    element
        .child_text(name)
        .and_then(|raw| number(&raw, name, warnings))
}

fn walk(
    body: &Element,
    path: &mut Vec<String>,
    items: &mut Vec<GaebBillItem>,
    warnings: &mut Vec<String>,
) {
    for category in body.children_named("BoQCtgy") {
        path.push(
            category
                .attr("RNoPart")
                .unwrap_or_default()
                .trim()
                .to_string(),
        );
        if let Some(inner) = category.child("BoQBody") {
            walk(inner, path, items, warnings);
        }
        path.pop();
    }
    for list in body.children_named("Itemlist") {
        for item in list.children_named("Item") {
            items.push(read_item(item, path, warnings));
        }
    }
}

fn read_item(item: &Element, path: &[String], warnings: &mut Vec<String>) -> GaebBillItem {
    let mut pieces = path.to_vec();
    pieces.push(item.attr("RNoPart").unwrap_or_default().trim().to_string());
    let (short_text, ..) = description(item.child("Description"));
    let takeoff = item.child("QtyDeterm").map(|determ| GaebTakeoff {
        qty: child_number(determ, "Qty", warnings),
        rows: determ
            .children_named("QDetermItem")
            .filter_map(|line| {
                let row = line.child("QTakeoff")?.attr("Row").unwrap_or_default();
                // Columns 1–11 (DA and REB ordinal) are not used in GAEB.
                let reb: String = row.chars().skip(11).collect();
                Some(GaebTakeoffRow {
                    row: reb.trim_end().to_string(),
                    explanation: line
                        .child_text("Explanation")
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                })
            })
            .collect(),
    });
    GaebBillItem {
        pieces,
        rno_index: item.attr("RNoIndex").unwrap_or_default().trim().to_string(),
        short_text,
        bill_qty: child_number(item, "BillQty", warnings)
            .or_else(|| child_number(item, "Qty", warnings)),
        unit: item.child_text("QU").unwrap_or_default().trim().to_string(),
        unit_price: child_number(item, "UP", warnings),
        total: child_number(item, "IT", warnings),
        lump_sum: item
            .child_text("LumpSumItem")
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("yes")),
        change_order: item
            .child_text("CONo")
            .unwrap_or_default()
            .trim()
            .to_string(),
        check_note: item
            .path(&["CheckNote", "Annotation"])
            .map(Element::text)
            .unwrap_or_default()
            .trim()
            .to_string(),
        takeoff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const X89: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA89/3.3">
  <GAEBInfo><Version>3.3</Version></GAEBInfo>
  <PrjInfo><NamePrj>Muster</NamePrj></PrjInfo>
  <Invoice>
    <DP>89</DP>
    <CTR><Address><Name1>Muster Regeltechnik GmbH</Name1><City>Musterstadt</City></Address></CTR>
    <BoQ>
      <BoQInfo><Totals><Total>1234.50</Total></Totals></BoQInfo>
      <BoQBody>
        <BoQCtgy RNoPart="01">
          <BoQBody>
            <Itemlist>
              <Item RNoPart="0010">
                <BillQty>12.000</BillQty><QU>St</QU><UP>100.00</UP><IT>1200.00</IT>
                <Description><OutlineText><OutlTxt><TextOutlTxt><span>Fühler</span></TextOutlTxt></OutlTxt></OutlineText></Description>
                <CheckNote><Annotation>2 Stück fehlen</Annotation></CheckNote>
                <QtyDeterm>
                  <Qty>12.000</Qty>
                  <QDetermItem><QTakeoff Row="           EG        0         10                              "/><BVBS:Explanation xmlns:BVBS="x">EG Flur Nord</BVBS:Explanation></QDetermItem>
                  <QDetermItem><QTakeoff Row="           OG        0          2                              "/></QDetermItem>
                </QtyDeterm>
              </Item>
              <Item RNoPart="0020"><BillQty>0.5</BillQty><QU>psch</QU><LumpSumItem>Yes</LumpSumItem><CONo>3</CONo></Item>
            </Itemlist>
          </BoQBody>
        </BoQCtgy>
      </BoQBody>
    </BoQ>
    <InvoiceHeader>
      <InvoiceNo>AR-2</InvoiceNo><InvoiceDate>2027-04-30</InvoiceDate><InvoiceType>deduction</InvoiceType>
      <SettlementType>accumulated</SettlementType><SequentialNo>2</SequentialNo>
    </InvoiceHeader>
  </Invoice>
</GAEB>"#;

    #[test]
    fn x89_header_items_and_takeoff() {
        let (billing, report) = read_billing(X89.as_bytes()).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(billing.phase, BillingPhase::Invoice);
        let header = billing.header.unwrap();
        assert_eq!(header.number, "AR-2");
        assert_eq!(header.kind, "deduction");
        assert_eq!(header.settlement, "accumulated");
        assert_eq!(header.sequential_no, Some(2));
        assert_eq!(billing.total, Some(Decimal::new(123450, 2)));
        assert_eq!(
            billing.contractor.unwrap().name1,
            "Muster Regeltechnik GmbH"
        );
        assert_eq!(billing.items.len(), 2);
        let first = &billing.items[0];
        assert_eq!(first.pieces, vec!["01".to_string(), "0010".to_string()]);
        assert_eq!(first.bill_qty, Some(Decimal::from(12)));
        assert_eq!(first.short_text, "Fühler");
        assert_eq!(first.check_note, "2 Stück fehlen");
        let takeoff = first.takeoff.as_ref().unwrap();
        assert_eq!(takeoff.qty, Some(Decimal::from(12)));
        assert_eq!(takeoff.rows.len(), 2);
        assert_eq!(takeoff.rows[0].explanation, "EG Flur Nord");
        assert!(
            takeoff.rows[1].row.starts_with("OG"),
            "{:?}",
            takeoff.rows[1].row
        );
        assert!(billing.items[1].lump_sum);
        assert_eq!(billing.items[1].change_order, "3");
    }

    #[test]
    fn x31_and_wrong_files() {
        let x31 = r#"<GAEB><QtyDeterm><DP>31</DP><BoQ><BoQBody><BoQCtgy RNoPart="1"><BoQBody><Itemlist>
            <Item RNoPart="10"><QtyDeterm><Qty>7,5</Qty></QtyDeterm></Item></Itemlist></BoQBody></BoQCtgy></BoQBody></BoQ></QtyDeterm></GAEB>"#;
        let (billing, _) = read_billing(x31.as_bytes()).unwrap();
        assert_eq!(billing.phase, BillingPhase::Takeoff);
        assert!(billing.header.is_none());
        assert_eq!(
            billing.items[0].takeoff.as_ref().unwrap().qty,
            Some(Decimal::new(75, 1))
        );
        let lv = "<GAEB><Award><DP>83</DP></Award></GAEB>";
        assert!(read_billing(lv.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("Leistungsverzeichnis"));
    }
}
