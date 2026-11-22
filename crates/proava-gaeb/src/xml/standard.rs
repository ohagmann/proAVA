//! Keys of standard texts in a Description: STLB-Bau (STLBBau) or an older
//! Standardleistungsnummer (StLNo). GAEB DA XML 3.1 to 3.4 use the same
//! elements; X84 has none (the bidder returns no texts).

use super::dom::Element;
use super::writer::escape;
use crate::model::{StandardNumber, StlbCharacteristic, StlbReference};

/// Text of a normalised string element as the writer leaves it: one line,
/// trimmed, at most `max` characters.
pub(crate) fn clean(text: &str, max: usize) -> String {
    let one_line = text.replace(['\n', '\r', '\t'], " ");
    let cut: String = one_line.trim().chars().take(max).collect();
    cut.trim_end().to_string()
}

/// `YYYY-MM` (xs:gYearMonth without time zone).
pub(crate) fn is_year_month(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 7
        && bytes[4] == b'-'
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[5..].iter().all(u8::is_ascii_digit)
        && matches!(
            &text[5..],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}

/// StLNo allows digits only, at most 17.
pub(crate) fn is_standard_number(text: &str) -> bool {
    text.len() <= 17 && text.chars().all(|ch| ch.is_ascii_digit())
}

fn integer(element: &Element, name: &str) -> Result<Option<i64>, String> {
    match element.child_text(name) {
        None => Ok(None),
        Some(text) => text
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{name} „{text}“ ist keine ganze Zahl")),
    }
}

/// Reads STLBBau and StLNo of a Description. Problems end up in `warnings`
/// (prefixed with `context`, e.g. the ordinal number); an STLBBau without a
/// usable Group is dropped because it cannot be written again.
pub(crate) fn read(
    description: &Element,
    context: &str,
    warnings: &mut Vec<String>,
) -> (Option<StlbReference>, Option<StandardNumber>) {
    let stlb = description
        .child("STLBBau")
        .and_then(|element| read_stlb(element, context, warnings));
    let number = description.child("StLNo").and_then(|element| {
        let number = element.text().trim().to_string();
        if number.is_empty() {
            return None;
        }
        Some(StandardNumber {
            number,
            catalog: element.attr("Type").unwrap_or_default().trim().to_string(),
        })
    });
    (stlb, number)
}

fn read_stlb(
    element: &Element,
    context: &str,
    warnings: &mut Vec<String>,
) -> Option<StlbReference> {
    let Some(catalog) = element.child("STLBBauCtlg") else {
        warnings.push(format!(
            "{context}: STLB-Bau-Kennung ohne STLBBauCtlg übergangen"
        ));
        return None;
    };
    let group = match integer(catalog, "Group") {
        Ok(Some(group)) => group,
        Ok(None) => {
            warnings.push(format!("{context}: STLB-Bau-Kennung ohne Group übergangen"));
            return None;
        }
        Err(message) => {
            warnings.push(format!("{context}: STLB-Bau-Kennung übergangen, {message}"));
            return None;
        }
    };
    let mut reference = StlbReference {
        catalog: catalog.child_text("CtlgName").unwrap_or_default(),
        version: catalog.child_text("VersDate"),
        work_category: catalog.child_text("WCtg").unwrap_or_default(),
        group,
        cost_group: catalog.child_text("CostGrp").unwrap_or_default(),
        characteristics: Vec::new(),
    };
    for id in element.children_named("STLBBauID") {
        let parsed = (|| {
            let characteristic =
                integer(id, "ArtChrIdent")?.ok_or_else(|| "ArtChrIdent fehlt".to_string())?;
            Ok::<_, String>(StlbCharacteristic {
                characteristic,
                index: integer(id, "ArtChIdx")?,
                value: integer(id, "ChVIdent")?,
                short_text_part: id
                    .child_text("OutlTextPart")
                    .is_some_and(|value| value.eq_ignore_ascii_case("yes")),
            })
        })();
        match parsed {
            Ok(characteristic) => reference.characteristics.push(characteristic),
            Err(message) => {
                warnings.push(format!("{context}: STLB-Bau-Merkmal übergangen, {message}"))
            }
        }
    }
    Some(reference)
}

/// The STLBBau or StLNo element for a Description (empty when there is
/// none). STLB-Bau wins: the schema allows only one of them.
pub(crate) fn write(stlb: Option<&StlbReference>, number: Option<&StandardNumber>) -> String {
    let mut out = String::new();
    if let Some(stlb) = stlb {
        out.push_str("<STLBBau><STLBBauCtlg>");
        leaf(&mut out, "CtlgName", &clean(&stlb.catalog, 10));
        if let Some(version) = stlb.version.as_deref().map(str::trim) {
            if is_year_month(version) {
                leaf(&mut out, "VersDate", version);
            }
        }
        let work_category = clean(&stlb.work_category, 3);
        if !work_category.is_empty() {
            leaf(&mut out, "WCtg", &work_category);
        }
        leaf(&mut out, "Group", &stlb.group.to_string());
        let cost_group = clean(&stlb.cost_group, 20);
        if !cost_group.is_empty() {
            leaf(&mut out, "CostGrp", &cost_group);
        }
        out.push_str("</STLBBauCtlg>");
        for id in &stlb.characteristics {
            out.push_str("<STLBBauID>");
            leaf(&mut out, "ArtChrIdent", &id.characteristic.to_string());
            if let Some(index) = id.index {
                leaf(&mut out, "ArtChIdx", &index.to_string());
            }
            if let Some(value) = id.value {
                leaf(&mut out, "ChVIdent", &value.to_string());
            }
            if id.short_text_part {
                leaf(&mut out, "OutlTextPart", "Yes");
            }
            out.push_str("</STLBBauID>");
        }
        out.push_str("</STLBBau>");
    } else if let Some(number) = number {
        let digits = number.number.trim();
        if !digits.is_empty() && is_standard_number(digits) {
            match standard_catalog(&number.catalog) {
                Some(kind) => out.push_str(&format!("<StLNo Type=\"{kind}\">{digits}</StLNo>")),
                None => out.push_str(&format!("<StLNo>{digits}</StLNo>")),
            }
        }
    }
    out
}

/// The Type attribute of StLNo as the schema spells it.
pub(crate) fn standard_catalog(catalog: &str) -> Option<&'static str> {
    let catalog = catalog.trim();
    ["STLB-BauZ", "StLB", "StLK"]
        .into_iter()
        .find(|kind| kind.eq_ignore_ascii_case(catalog))
}

fn leaf(out: &mut String, tag: &str, value: &str) {
    if value.is_empty() {
        out.push_str(&format!("<{tag}/>"));
    } else {
        out.push_str(&format!("<{tag}>{}</{tag}>", escape(value)));
    }
}

/// What reading the written Description gives back (see [`write`]).
pub(crate) fn as_written(stlb: &mut Option<StlbReference>, number: &mut Option<StandardNumber>) {
    if let Some(reference) = stlb {
        reference.catalog = clean(&reference.catalog, 10);
        reference.version = reference
            .version
            .as_deref()
            .map(str::trim)
            .filter(|version| is_year_month(version))
            .map(str::to_string);
        reference.work_category = clean(&reference.work_category, 3);
        reference.cost_group = clean(&reference.cost_group, 20);
        *number = None;
    } else if let Some(standard) = number {
        let digits = standard.number.trim().to_string();
        if digits.is_empty() || !is_standard_number(&digits) {
            *number = None;
        } else {
            standard.number = digits;
            standard.catalog = standard_catalog(&standard.catalog)
                .unwrap_or_default()
                .to_string();
        }
    }
}
