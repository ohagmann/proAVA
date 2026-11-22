//! Positionsbibliothek in `projekt.xml` (schema 2, optional – older files
//! simply have none): an entry's Stichwörter, Stand and price date, and the
//! origin of a position taken from a library.
//!
//! ```xml
//! <library keywords="KNX; Raumbedienung" as-of="01.11.2026" price-date="10/2026"/>
//! <origin library="proAVA-Bibliothek" entry="42" as-of="01.11.2026"/>
//! ```

use proava_domain::library::{EntryInfo, LibraryOrigin};
use proava_domain::{Position, RowId};

use crate::xml_tree::Element;

/// Adds library data of `position` to its element.
pub(crate) fn with_library_data(mut element: Element, position: &Position) -> Element {
    let info = position.library_info();
    if !info.is_empty() {
        let keywords = (!info.keywords.is_empty()).then(|| info.keywords.join("; "));
        element.push(
            Element::new("library")
                .attr_opt("keywords", keywords)
                .attr_opt("as-of", non_empty(&info.as_of))
                .attr_opt("price-date", non_empty(&info.price_date)),
        );
    }
    if let Some(origin) = position.origin() {
        element.push(
            Element::new("origin")
                .attr("library", origin.library.as_str())
                .attr("entry", origin.entry.0.to_string())
                .attr_opt("as-of", non_empty(&origin.as_of)),
        );
    }
    element
}

/// Reads the library data of a `<position>` element.
pub(crate) fn read_library_data(node: &Element, position: &mut Position) -> Result<(), String> {
    if let Some(library) = node.first("library") {
        position.set_library_info(EntryInfo {
            keywords: EntryInfo::parse_keywords(&library.text("keywords")),
            as_of: library.text("as-of"),
            price_date: library.text("price-date"),
        });
    }
    // `<origin>` with an entry; a text source of an older AUSSCHREIBEN.DE
    // build (`<origin source=…>`, see `project_stlb`) is not a library link.
    if let Some(origin) = node
        .all("origin")
        .find(|origin| origin.get("entry").is_some())
    {
        let entry = origin.text("entry").trim().parse::<u64>().map_err(|_| {
            format!(
                "Herkunft aus der Bibliothek: „{}“ ist keine Nummer.",
                origin.text("entry")
            )
        })?;
        position.set_origin(Some(LibraryOrigin {
            library: origin.text("library"),
            entry: RowId(entry),
            as_of: origin.text("as-of"),
        }));
    }
    Ok(())
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}
