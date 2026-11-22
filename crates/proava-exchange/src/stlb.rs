//! STLB-Bau key and Standardleistungsnummer: domain ↔ GAEB exchange model,
//! and taking positions from a GAEB file written by STLB-Bau.

use proava_application::{CatalogKind, CatalogPositions};
use std::path::Path;

use proava_domain::text_origin::{reference_in_text, source_of, supplier_in_text};
use proava_domain::{
    CatalogKeys, ComplementKind, ComplementNumber, LvRow, Note, Position, Project, RowKind,
    StandardNumber, StlbCharacteristic, StlbReference, TextAnchor, TextComplement, TextOrigin,
};
use proava_gaeb as gaeb;

/// GAEB item → keys of the domain position. Call after texts and
/// complements are set: the text fingerprint is taken from the position.
pub(crate) fn keys_from_gaeb(item: &gaeb::Item, position: &mut Position) {
    if let Some(stlb) = &item.stlb {
        let complement_links = item
            .complements
            .iter()
            .filter_map(|complement| {
                complement
                    .stlb_characteristic
                    .map(|ident| (complement.marker.trim().to_string(), ident))
            })
            .collect();
        let complement_numbers = item
            .complements
            .iter()
            .filter_map(|complement| {
                let number = match complement.number? {
                    gaeb::ComplementNumber::Decimal => ComplementNumber::Decimal,
                    gaeb::ComplementNumber::Integer => ComplementNumber::Integer,
                };
                Some((complement.marker.trim().to_string(), number))
            })
            .collect();
        position.attach_stlb(StlbReference {
            owner_complements: item.complements.iter().any(|c| c.kind == "Owner"),
            bidder_complements: item.complements.iter().any(|c| c.kind != "Owner"),
            complement_links,
            complement_numbers,
            ..stlb_from_gaeb(stlb)
        });
    }
    position.set_standard_number(item.standard_number.as_ref().map(|number| StandardNumber {
        number: number.number.clone(),
        catalog: number.catalog.clone(),
    }));
}

/// Keys of the domain position → GAEB item (after the complements are set).
/// A Langtext changed after the take-over is no STLB-Bau text any more: the
/// position goes out as a free text, without `STLBBau` (the export names it).
/// A changed Kurztext keeps the key – STLB-Bau's Kurztext is only the short
/// address of the Teilleistung, the Langtext is the description.
pub(crate) fn keys_to_gaeb(position: &Position, item: &mut gaeb::Item) {
    if position.stlb_text_changed() {
        return;
    }
    if let Some(stlb) = position.stlb_reference() {
        item.stlb = catalog_to_gaeb(&CatalogKeys {
            stlb: Some(stlb.clone()),
            standard_number: None,
        })
        .0;
        for complement in &mut item.complements {
            let marker = complement.marker.trim();
            complement.stlb_characteristic = stlb
                .complement_links
                .iter()
                .find(|(linked, _)| linked.trim() == marker)
                .map(|(_, ident)| *ident);
            complement.number = stlb
                .complement_numbers
                .iter()
                .find(|(numbered, _)| numbered.trim() == marker)
                .map(|(_, number)| match number {
                    ComplementNumber::Decimal => gaeb::ComplementNumber::Decimal,
                    ComplementNumber::Integer => gaeb::ComplementNumber::Integer,
                });
        }
    }
    item.standard_number = position
        .standard_number()
        .map(|number| gaeb::StandardNumber {
            number: number.number.clone(),
            catalog: number.catalog.clone(),
        });
}

/// The positions of a project read from a catalogue file, without images.
/// Hinweistexte and Ausführungsbeschreibungen with a catalogue key (STLB-Bau
/// Standardbeschreibungen) come along in position form, so that the list and
/// the take-over treat every entry alike; [`CatalogKind`] says what they were.
pub(crate) fn catalog_positions(project: &Project, mut warnings: Vec<String>) -> CatalogPositions {
    let mut positions = Vec::new();
    let mut kinds = Vec::new();
    let mut images = 0;
    for row in project.lv().rows() {
        let delivered = match row {
            LvRow::Position(_) => RowKind::Position,
            LvRow::Note(note) if !note.catalog().is_empty() => RowKind::Note,
            LvRow::Performance(performance) if !performance.catalog().is_empty() => {
                RowKind::Performance
            }
            _ => continue,
        };
        let mut position = match row {
            LvRow::Position(position) => position.clone(),
            text => match proava_domain::row_kind::converted(text, RowKind::Position) {
                Some(LvRow::Position(position)) => position,
                _ => continue,
            },
        };
        if position.long_text().has_images() {
            images += 1;
            let without = position.long_text().without_images();
            let had_key = position.stlb_reference().cloned();
            position.set_long_text(without);
            if let Some(stlb) = had_key {
                // The key belongs to the text as delivered, minus the images.
                position.attach_stlb(stlb);
            }
        }
        let suggested = if delivered == RowKind::Position && looks_like_text(&position) {
            RowKind::Note
        } else {
            delivered
        };
        kinds.push(CatalogKind {
            delivered,
            suggested,
        });
        positions.push(position);
    }
    if images > 0 {
        warnings.push(format!("Bilder in {images} Langtext(en) nicht übernommen."));
    }
    let with_stlb_key = positions
        .iter()
        .filter(|position| position.stlb_reference().is_some())
        .count();
    if positions.is_empty() {
        warnings.push("Die Datei enthält keine Positionen.".to_string());
    }
    CatalogPositions {
        positions,
        kinds,
        with_stlb_key,
        warnings,
        source_in_archive: String::new(),
    }
}

/// An Item that is really a text: no unit and no quantity (so STLB-Bau
/// Online delivers a Standardbeschreibung, Anwender 18.11.). Manufacturer texts
/// come with a unit and stay positions.
pub(crate) fn looks_like_text(position: &Position) -> bool {
    position.unit().trim().is_empty()
        && position.quantity().is_zero()
        && position.unit_price().is_none()
}

/// What a catalogue file says about itself, for the Herkunft of its texts.
pub(crate) struct Delivered<'a> {
    /// GAEBInfo/ProgName (or ProgSystem) of the file.
    pub program: &'a str,
    /// Name of the file (inside a zip: „Archiv.zip → LV.X83“).
    pub file_name: &'a str,
    /// Names of the delivered LV, most telling first (LV label, project label …).
    pub lv_names: [&'a str; 4],
    /// Day the file was written (downloaded), ISO.
    pub retrieved: &'a str,
}

/// Sets the Herkunft of every position read from a catalogue file: source
/// from the program or file name (empty when the file does not tell – the
/// dialog fills in its source), manufacturer from a „Hersteller:“ line of the
/// text or else the name of the delivered LV, article number from the text.
pub(crate) fn mark_origin(positions: &mut [Position], delivered: &Delivered) {
    let lv_name = delivered
        .lv_names
        .iter()
        .map(|name| name.trim())
        .find(|name| !name.is_empty())
        .unwrap_or_default();
    for position in positions {
        let text = format!(
            "{}\n{}",
            position.short_text(),
            position.long_text().to_plain()
        );
        let source = source_of(
            delivered.program,
            delivered.file_name,
            position.stlb_reference().is_some(),
            "",
        );
        position.set_text_origin(Some(TextOrigin {
            source,
            supplier: supplier_in_text(&text).unwrap_or_else(|| lv_name.to_string()),
            retrieved: delivered.retrieved.to_string(),
            file: delivered.file_name.to_string(),
            reference: reference_in_text(&text).unwrap_or_default(),
            note: String::new(),
        }));
    }
}

/// File name for the Herkunft: „LV.X83“ or „Archiv.zip → LV.X83“.
pub(crate) fn file_name_of(file: &Path, in_archive: &str) -> String {
    let name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if in_archive.is_empty() {
        name
    } else {
        format!("{name} → {in_archive}")
    }
}

/// Day the file was last written (local time), ISO; empty if unknown.
pub(crate) fn modified_day(file: &Path) -> String {
    std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .map(|modified| {
            chrono::DateTime::<chrono::Local>::from(modified)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_default()
}

fn stlb_from_gaeb(stlb: &gaeb::StlbReference) -> StlbReference {
    StlbReference {
        catalog: stlb.catalog.clone(),
        version: stlb.version.clone(),
        work_category: stlb.work_category.clone(),
        group: stlb.group,
        cost_group: stlb.cost_group.clone(),
        characteristics: stlb
            .characteristics
            .iter()
            .map(|id| StlbCharacteristic {
                characteristic: id.characteristic,
                index: id.index,
                value: id.value,
                short_text_part: id.short_text_part,
            })
            .collect(),
        ..StlbReference::default()
    }
}

/// Keys of a GAEB sub or performance description → domain.
pub(crate) fn catalog_from_gaeb(
    stlb: Option<&gaeb::StlbReference>,
    number: Option<&gaeb::StandardNumber>,
) -> CatalogKeys {
    CatalogKeys {
        stlb: stlb.map(stlb_from_gaeb),
        standard_number: number.map(|number| StandardNumber {
            number: number.number.clone(),
            catalog: number.catalog.clone(),
        }),
    }
}

/// Domain keys of a sub or performance description → GAEB.
pub(crate) fn catalog_to_gaeb(
    keys: &CatalogKeys,
) -> (Option<gaeb::StlbReference>, Option<gaeb::StandardNumber>) {
    let stlb = keys.stlb.as_ref().map(|stlb| gaeb::StlbReference {
        catalog: stlb.catalog.clone(),
        version: stlb.version.clone(),
        work_category: stlb.work_category.clone(),
        group: stlb.group,
        cost_group: stlb.cost_group.clone(),
        characteristics: stlb
            .characteristics
            .iter()
            .map(|id| gaeb::StlbCharacteristic {
                characteristic: id.characteristic,
                index: id.index,
                value: id.value,
                short_text_part: id.short_text_part,
            })
            .collect(),
    });
    let number = keys
        .standard_number
        .as_ref()
        .map(|number| gaeb::StandardNumber {
            number: number.number.clone(),
            catalog: number.catalog.clone(),
        });
    (stlb, number)
}

/// GAEB Hinweistext (Remark) → Textergänzungen and catalogue keys of the
/// note; the STLB-Bau key gets TA/TB and the links of its complements.
pub(crate) fn note_from_remark(remark: &gaeb::Remark, note: &mut Note) {
    let (complements, keys) = text_from_gaeb(
        &remark.complements,
        remark.stlb.as_ref(),
        remark.standard_number.as_ref(),
    );
    note.set_complements(complements);
    note.set_catalog(keys);
}

/// Textergänzungen and keys of a GAEB text (Remark, PerfDescr) for the
/// domain; the STLB-Bau key gets TA/TB and the links of its complements.
pub(crate) fn text_from_gaeb(
    complements: &[gaeb::TextComplement],
    stlb: Option<&gaeb::StlbReference>,
    number: Option<&gaeb::StandardNumber>,
) -> (Vec<TextComplement>, CatalogKeys) {
    let list = complements
        .iter()
        .map(|complement| TextComplement {
            marker: complement.marker.trim().to_string(),
            kind: if complement.kind == "Owner" {
                ComplementKind::Owner
            } else {
                ComplementKind::Bidder
            },
            caption: complement.caption.clone(),
            body: complement.body.clone(),
            tail: complement.tail.clone(),
            anchor: complement.anchor.map(|anchor| TextAnchor {
                block: anchor.block,
                offset: anchor.offset,
                own_line: anchor.own_line,
            }),
        })
        .collect();
    let mut keys = catalog_from_gaeb(stlb, number);
    if let Some(stlb) = &mut keys.stlb {
        link_from_gaeb(complements, stlb);
    }
    (list, keys)
}

/// Note → Textergänzungen and keys of the GAEB Remark.
pub(crate) fn remark_from_note(note: &Note, remark: &mut gaeb::Remark) {
    let (complements, stlb, number) = text_to_gaeb(note.complements(), note.catalog());
    remark.complements = complements;
    remark.stlb = stlb;
    remark.standard_number = number;
}

/// Textergänzungen and keys of a domain text for GAEB (Remark, PerfDescr).
pub(crate) fn text_to_gaeb(
    complements: &[TextComplement],
    catalog: &CatalogKeys,
) -> (
    Vec<gaeb::TextComplement>,
    Option<gaeb::StlbReference>,
    Option<gaeb::StandardNumber>,
) {
    let mut list: Vec<gaeb::TextComplement> = complements
        .iter()
        .map(|complement| gaeb::TextComplement {
            marker: complement.marker.clone(),
            kind: match complement.kind {
                ComplementKind::Owner => "Owner",
                ComplementKind::Bidder => "Bidder",
            }
            .to_string(),
            caption: complement.caption.clone(),
            body: complement.body.clone(),
            tail: complement.tail.clone(),
            anchor: complement.anchor.map(|anchor| gaeb::TextAnchor {
                block: anchor.block,
                offset: anchor.offset,
                own_line: anchor.own_line,
            }),
            ..gaeb::TextComplement::default()
        })
        .collect();
    let (stlb, number) = catalog_to_gaeb(catalog);
    if let Some(reference) = &catalog.stlb {
        link_to_gaeb(reference, &mut list);
    }
    (list, stlb, number)
}

/// TA/TB, characteristic links and numeric kinds of the complements.
fn link_from_gaeb(complements: &[gaeb::TextComplement], stlb: &mut StlbReference) {
    stlb.owner_complements = complements.iter().any(|c| c.kind == "Owner");
    stlb.bidder_complements = complements.iter().any(|c| c.kind != "Owner");
    stlb.complement_links = complements
        .iter()
        .filter_map(|c| {
            c.stlb_characteristic
                .map(|id| (c.marker.trim().to_string(), id))
        })
        .collect();
    stlb.complement_numbers = complements
        .iter()
        .filter_map(|c| {
            let number = match c.number? {
                gaeb::ComplementNumber::Decimal => ComplementNumber::Decimal,
                gaeb::ComplementNumber::Integer => ComplementNumber::Integer,
            };
            Some((c.marker.trim().to_string(), number))
        })
        .collect();
}

fn link_to_gaeb(stlb: &StlbReference, complements: &mut [gaeb::TextComplement]) {
    for complement in complements {
        let marker = complement.marker.trim();
        complement.stlb_characteristic = stlb
            .complement_links
            .iter()
            .find(|(linked, _)| linked.trim() == marker)
            .map(|(_, ident)| *ident);
        complement.number = stlb
            .complement_numbers
            .iter()
            .find(|(numbered, _)| numbered.trim() == marker)
            .map(|(_, number)| match number {
                ComplementNumber::Decimal => gaeb::ComplementNumber::Decimal,
                ComplementNumber::Integer => gaeb::ComplementNumber::Integer,
            });
    }
}
