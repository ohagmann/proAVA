//! STLB-Bau key and Standardleistungsnummer of a position in `projekt.xml`
//! (schema 2, optional children of `<position>` – older files simply have none):
//!
//! ```xml
//! <stlb catalog="STLB-Bau" version="2026-04" lb="070" group="9001" cost-group="481"
//!       ta="1" tb="1" text="9f3c…">
//!   <char id="90011" index="0" value="2" short="1"/>
//!   <complement marker="1" char="90013"/>
//! </stlb>
//! <standard-number number="12345678901234567" catalog="StLB"/>
//! <text-origin source="AUSSCHREIBEN.DE" supplier="Muster GmbH" date="2026-11-17"
//!              file="Merkliste.X83" ref="MA-24"/>
//! ```
//!
//! `<text-origin>` (Textquelle: where the text came from) is written for
//! positions, hints and Ausführungsbeschreibungen (a row may change its kind).
//! `<origin library=… entry=…>` is something else – the library
//! entry a position was taken from (`project_library`). Files of the first
//! AUSSCHREIBEN.DE build wrote the text source as `<origin source=…>`
//! without `entry`; they are still read.

use proava_domain::{
    CatalogKeys, ComplementNumber, Position, StandardNumber, StlbCharacteristic, StlbReference,
    TextOrigin,
};

use crate::xml_tree::Element;

/// Adds the catalogue keys and the Herkunft of `position` to its element.
pub(crate) fn with_catalog_keys(element: Element, position: &Position) -> Element {
    let element = with_keys(
        element,
        position.stlb_reference(),
        position.standard_number(),
    );
    with_text_origin(element, position.text_origin())
}

/// Adds `<text-origin>` when the text has one.
pub(crate) fn with_text_origin(mut element: Element, origin: Option<&TextOrigin>) -> Element {
    if let Some(origin) = origin {
        element.push(
            Element::new("text-origin")
                .attr("source", origin.source.as_str())
                .attr("supplier", origin.supplier.as_str())
                .attr("date", origin.retrieved.as_str())
                .attr("file", origin.file.as_str())
                .attr("ref", origin.reference.as_str())
                .attr_opt(
                    "note",
                    (!origin.note.trim().is_empty()).then(|| origin.note.clone()),
                ),
        );
    }
    element
}

/// Adds catalogue keys of a sub or performance description to its element.
pub(crate) fn with_keys_of(element: Element, keys: &CatalogKeys) -> Element {
    with_keys(element, keys.stlb.as_ref(), keys.standard_number.as_ref())
}

fn with_keys(
    mut element: Element,
    stlb: Option<&StlbReference>,
    number: Option<&StandardNumber>,
) -> Element {
    if let Some(stlb) = stlb {
        let mut node = Element::new("stlb")
            .attr("catalog", stlb.catalog.as_str())
            .attr_opt("version", stlb.version.clone())
            .attr("lb", stlb.work_category.as_str())
            .attr("group", stlb.group.to_string())
            .attr("cost-group", stlb.cost_group.as_str())
            .flag("ta", stlb.owner_complements)
            .flag("tb", stlb.bidder_complements)
            // Fingerprint of the Langtext; `text` is the earlier one over
            // Kurz- and Langtext, only kept when it no longer matched.
            .attr_opt(
                "long-text",
                stlb.text_fingerprint
                    .map(|fingerprint| format!("{fingerprint:016x}")),
            )
            .attr_opt(
                "text",
                stlb.legacy_fingerprint
                    .map(|fingerprint| format!("{fingerprint:016x}")),
            );
        for id in &stlb.characteristics {
            node.push(
                Element::new("char")
                    .attr("id", id.characteristic.to_string())
                    .attr_opt("index", id.index.map(|index| index.to_string()))
                    .attr_opt("value", id.value.map(|value| value.to_string()))
                    .flag("short", id.short_text_part),
            );
        }
        for (marker, characteristic) in &stlb.complement_links {
            let number = stlb
                .complement_numbers
                .iter()
                .find(|(numbered, _)| numbered == marker)
                .map(|(_, number)| number.key());
            node.push(
                Element::new("complement")
                    .attr("marker", marker.as_str())
                    .attr("char", characteristic.to_string())
                    .attr_opt("number", number),
            );
        }
        // Numeric complements without a characteristic link (rare).
        for (marker, number) in &stlb.complement_numbers {
            if !stlb
                .complement_links
                .iter()
                .any(|(linked, _)| linked == marker)
            {
                node.push(
                    Element::new("complement")
                        .attr("marker", marker.as_str())
                        .attr("number", number.key()),
                );
            }
        }
        element.push(node);
    }
    if let Some(number) = number {
        element.push(
            Element::new("standard-number")
                .attr("number", number.number.as_str())
                .attr("catalog", number.catalog.as_str()),
        );
    }
    element
}

/// Reads the catalogue keys of a `<position>` element into `position`.
pub(crate) fn read_catalog_keys(node: &Element, position: &mut Position) -> Result<(), String> {
    let keys = read_keys(node)?;
    position.set_stlb_reference(keys.stlb);
    // Files with the fingerprint over Kurz- and Langtext: switch to the
    // Langtext-only one where the text is still as delivered.
    position.upgrade_stlb_fingerprint();
    position.set_standard_number(keys.standard_number);
    let text_origin = read_text_origin(node).or_else(|| {
        // Older form: `<origin source=…>` without a library entry.
        node.all("origin")
            .find(|origin| origin.get("source").is_some() && origin.get("entry").is_none())
            .map(text_origin_of)
    });
    position.set_text_origin(text_origin);
    Ok(())
}

/// `<text-origin>` below `node`, if any.
pub(crate) fn read_text_origin(node: &Element) -> Option<TextOrigin> {
    node.first("text-origin").map(text_origin_of)
}

fn text_origin_of(origin: &Element) -> TextOrigin {
    TextOrigin {
        source: origin.text("source"),
        supplier: origin.text("supplier"),
        retrieved: origin.text("date"),
        file: origin.text("file"),
        reference: origin.text("ref"),
        note: origin.text("note"),
    }
}

/// Reads the catalogue keys below `node` (`<sub>`, `<performance>`, `<position>`).
pub(crate) fn read_keys(node: &Element) -> Result<CatalogKeys, String> {
    let mut keys = CatalogKeys::default();
    if let Some(stlb) = node.first("stlb") {
        let integer = |element: &Element, key: &str| -> Result<Option<i64>, String> {
            element
                .get(key)
                .map(|text| {
                    text.trim().parse::<i64>().map_err(|_| {
                        format!("STLB-Bau-Kennung: „{text}“ ist keine ganze Zahl ({key})")
                    })
                })
                .transpose()
        };
        let mut reference = StlbReference {
            catalog: stlb.text("catalog"),
            version: stlb.get("version").map(str::to_string),
            work_category: stlb.text("lb"),
            group: integer(stlb, "group")?.unwrap_or(0),
            cost_group: stlb.text("cost-group"),
            characteristics: Vec::new(),
            owner_complements: stlb.is_set("ta"),
            bidder_complements: stlb.is_set("tb"),
            text_fingerprint: stlb
                .get("long-text")
                .and_then(|hex| u64::from_str_radix(hex.trim(), 16).ok()),
            legacy_fingerprint: stlb
                .get("text")
                .and_then(|hex| u64::from_str_radix(hex.trim(), 16).ok()),
            complement_links: Vec::new(),
            complement_numbers: Vec::new(),
        };
        for link in stlb.all("complement") {
            if let Some(characteristic) = integer(link, "char")? {
                reference
                    .complement_links
                    .push((link.text("marker"), characteristic));
            }
            if let Some(number) = link.get("number").and_then(ComplementNumber::parse) {
                reference
                    .complement_numbers
                    .push((link.text("marker"), number));
            }
        }
        for id in stlb.all("char") {
            reference.characteristics.push(StlbCharacteristic {
                characteristic: integer(id, "id")?.unwrap_or(0),
                index: integer(id, "index")?,
                value: integer(id, "value")?,
                short_text_part: id.is_set("short"),
            });
        }
        keys.stlb = Some(reference);
    }
    if let Some(number) = node.first("standard-number") {
        keys.standard_number = Some(StandardNumber {
            number: number.text("number"),
            catalog: number.text("catalog"),
        });
    }
    Ok(keys)
}
