//! Abweichungsprotokoll eines Angebots (`<structure>` in `<bid>`): Positionen
//! der Datei ohne LV-Position, Zuordnungen, Konflikte, Vorschläge und die
//! Abweichungen je Position (siehe `proava_domain::bid_structure`).

use proava_domain::{
    AssignedBy, Assignment, BidItem, Money, RowId, StoredSuggestion, StructureReport,
};

use crate::project_xml::{number, optional_decimal};
use crate::xml_tree::Element;

fn item_element(name: &str, item: &BidItem) -> Element {
    let text = |value: &str| (!value.is_empty()).then(|| value.to_string());
    Element::new(name)
        .attr("oz", item.ordinal.as_str())
        .attr_opt("t", text(&item.short_text))
        .attr_opt("long", text(&item.long_text))
        .attr_opt("unit", text(&item.unit))
        .attr_opt("quantity", item.quantity.map(|value| value.to_string()))
        .attr_opt("unit-price", item.unit_price.map(|value| value.to_string()))
        .attr_opt("total", item.total.map(|value| value.to_string()))
        .flag("not-offered", item.not_offered)
        .attr_opt("kind", text(&item.kind))
}

fn item_of(node: &Element) -> Result<BidItem, String> {
    Ok(BidItem {
        ordinal: node.text("oz"),
        short_text: node.text("t"),
        long_text: node.text("long"),
        unit: node.text("unit"),
        quantity: optional_decimal(node, "quantity", "Menge")?,
        unit_price: optional_decimal(node, "unit-price", "Einheitspreis")?,
        total: optional_decimal(node, "total", "Gesamtpreis")?,
        not_offered: node.is_set("not-offered"),
        kind: node.text("kind"),
    })
}

fn row_of(node: &Element) -> Result<Option<RowId>, String> {
    Ok(number::<u64>(node, "row")?.map(RowId))
}

/// The element for a bid's report; None when there is nothing to keep.
pub(crate) fn write(report: &StructureReport) -> Option<Element> {
    if report.is_empty() && report.suggestions.is_empty() && report.file_total.is_none() {
        return None;
    }
    let mut element = Element::new("structure").attr_opt(
        "file-total",
        report
            .file_total
            .map(|total| total.to_decimal().to_string()),
    );
    for item in &report.foreign {
        element.push(item_element("foreign", item));
    }
    for assignment in &report.assigned {
        element.push(
            item_element("assigned", &assignment.item)
                .attr("row", assignment.row.0.to_string())
                .attr("by", assignment.by.key())
                .attr("similarity", assignment.similarity.to_string()),
        );
    }
    for (row, item) in &report.conflicts {
        element.push(item_element("conflict", item).attr("row", row.0.to_string()));
    }
    for suggestion in &report.suggestions {
        element.push(
            Element::new("suggestion")
                .attr("oz", suggestion.ordinal.as_str())
                .attr("row", suggestion.row.0.to_string())
                .attr("similarity", suggestion.similarity.to_string())
                .flag("sure", suggestion.sure),
        );
    }
    let pairs = |name: &str, list: Vec<(RowId, String)>| {
        list.into_iter()
            .map(|(row, value)| {
                Element::new(name)
                    .attr("row", row.0.to_string())
                    .attr("value", value)
            })
            .collect::<Vec<_>>()
    };
    for child in pairs(
        "quantity",
        report
            .quantities
            .iter()
            .map(|(row, value)| (*row, value.to_string()))
            .collect(),
    )
    .into_iter()
    .chain(pairs("unit", report.units.clone()))
    .chain(pairs("short", report.short_texts.clone()))
    .chain(pairs(
        "long",
        report
            .long_texts
            .iter()
            .map(|(row, value)| (*row, value.to_string()))
            .collect(),
    ))
    .chain(pairs("kind", report.kinds.clone()))
    {
        element.push(child);
    }
    for title in &report.titles {
        element.push(Element::new("title").attr("t", title.as_str()));
    }
    Some(element)
}

/// Reads [`write`]; a missing element is an empty report.
pub(crate) fn read(node: Option<&Element>) -> Result<StructureReport, String> {
    let Some(node) = node else {
        return Ok(StructureReport::default());
    };
    let mut report = StructureReport {
        file_total: optional_decimal(node, "file-total", "Dateisumme")?.map(Money::from_decimal),
        ..StructureReport::default()
    };
    for item in node.all("foreign") {
        report.foreign.push(item_of(item)?);
    }
    for item in node.all("assigned") {
        if let Some(row) = row_of(item)? {
            report.assigned.push(Assignment {
                item: item_of(item)?,
                row,
                by: AssignedBy::from_key(&item.text("by")),
                similarity: number::<u8>(item, "similarity")?.unwrap_or(0),
            });
        }
    }
    for item in node.all("conflict") {
        if let Some(row) = row_of(item)? {
            report.conflicts.push((row, item_of(item)?));
        }
    }
    for item in node.all("suggestion") {
        if let Some(row) = row_of(item)? {
            report.suggestions.push(StoredSuggestion {
                ordinal: item.text("oz"),
                row,
                similarity: number::<u8>(item, "similarity")?.unwrap_or(0),
                sure: item.is_set("sure"),
            });
        }
    }
    for item in node.all("quantity") {
        if let (Some(row), Some(value)) = (row_of(item)?, optional_decimal(item, "value", "Menge")?)
        {
            report.quantities.push((row, value));
        }
    }
    let texts = |name: &str| -> Result<Vec<(RowId, String)>, String> {
        let mut list = Vec::new();
        for item in node.all(name) {
            if let Some(row) = row_of(item)? {
                list.push((row, item.text("value")));
            }
        }
        Ok(list)
    };
    report.units = texts("unit")?;
    report.short_texts = texts("short")?;
    report.kinds = texts("kind")?;
    for item in node.all("long") {
        if let (Some(row), Some(value)) = (row_of(item)?, number::<u8>(item, "value")?) {
            report.long_texts.push((row, value));
        }
    }
    report.titles = node.all("title").map(|title| title.text("t")).collect();
    Ok(report)
}
