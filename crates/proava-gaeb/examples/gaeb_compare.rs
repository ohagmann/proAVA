//! Reads two GAEB files of the same LV (e.g. the X84 and D84 of one bid)
//! with the matching codec and compares them item by item: ordinal, unit
//! price, total price, quantity, kinds and text complements. Only reads.
//!
//! ```sh
//! cargo run -p proava-gaeb --example gaeb_compare -- angebot.x84 angebot.d84
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use proava_gaeb::{datan, xml, GaebDocument, Item};

fn read(path: &Path) -> Result<GaebDocument, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension.starts_with('x') {
        xml::read_file(path).map_err(|error| error.to_string())
    } else {
        datan::read_file(path)
            .map(|report| report.document)
            .map_err(|error| error.to_string())
    }
}

fn items(doc: &GaebDocument) -> BTreeMap<String, Item> {
    doc.award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, item)| (pieces.join("."), item.clone()))
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [left_path, right_path] = args.as_slice() else {
        eprintln!("Aufruf: gaeb_compare <datei1> <datei2>");
        std::process::exit(2);
    };
    let (left, right) = match (read(Path::new(left_path)), read(Path::new(right_path))) {
        (Ok(left), Ok(right)) => (left, right),
        (left, right) => {
            if let Err(error) = left {
                eprintln!("{left_path}: {error}");
            }
            if let Err(error) = right {
                eprintln!("{right_path}: {error}");
            }
            std::process::exit(1);
        }
    };
    let a = items(&left);
    let b = items(&right);
    let only_left: Vec<&String> = a.keys().filter(|key| !b.contains_key(*key)).collect();
    let only_right: Vec<&String> = b.keys().filter(|key| !a.contains_key(*key)).collect();
    println!(
        "Positionen: {} / {}, nur links {}, nur rechts {}",
        a.len(),
        b.len(),
        only_left.len(),
        only_right.len()
    );
    for key in only_left.iter().take(5) {
        println!("   nur links: {key}");
    }
    for key in only_right.iter().take(5) {
        println!("   nur rechts: {key}");
    }
    let mut differences: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (key, x) in &a {
        let Some(y) = b.get(key) else { continue };
        let mut check = |field: &'static str, equal: bool, detail: String| {
            if !equal {
                differences
                    .entry(field)
                    .or_default()
                    .push(format!("{key}: {detail}"));
            }
        };
        check(
            "EP",
            x.unit_price == y.unit_price,
            format!("{:?} ≠ {:?}", x.unit_price, y.unit_price),
        );
        check(
            "GP",
            x.total == y.total,
            format!("{:?} ≠ {:?}", x.total, y.total),
        );
        check(
            "Menge",
            x.qty == y.qty,
            format!("{:?} ≠ {:?}", x.qty, y.qty),
        );
        check(
            "Preisanteile",
            x.up_components == y.up_components,
            format!("{:?} ≠ {:?}", x.up_components, y.up_components),
        );
        check(
            "Nachlass",
            x.discount_percent == y.discount_percent,
            format!("{:?} ≠ {:?}", x.discount_percent, y.discount_percent),
        );
        check(
            "Zuschlag %",
            x.markup_percent == y.markup_percent,
            format!("{:?} ≠ {:?}", x.markup_percent, y.markup_percent),
        );
        check(
            "Zuschlagsbasis",
            x.markup_base == y.markup_base,
            format!("{:?} ≠ {:?}", x.markup_base, y.markup_base),
        );
        check(
            "nicht angeboten",
            x.not_offered == y.not_offered,
            format!("{} ≠ {}", x.not_offered, y.not_offered),
        );
        check(
            "Positionsart",
            x.kind == y.kind,
            format!("{:?} ≠ {:?}", x.kind, y.kind),
        );
        let complements = |item: &Item| -> Vec<(String, String, String)> {
            item.complements
                .iter()
                .map(|complement| {
                    (
                        complement.marker.clone(),
                        complement.kind.clone(),
                        complement.body.clone(),
                    )
                })
                .collect()
        };
        check(
            "Textergänzungen",
            complements(x) == complements(y),
            format!("{:?} ≠ {:?}", complements(x), complements(y)),
        );
        check(
            "Kurztext",
            x.short_text == y.short_text,
            format!("{:?} ≠ {:?}", x.short_text, y.short_text),
        );
    }
    if differences.is_empty() {
        println!("Alle gemeinsamen Positionen stimmen überein (EP, GP, Menge, Anteile, Arten, Textergänzungen, Kurztext).");
    }
    for (field, list) in &differences {
        println!("{field}: {} Abweichungen", list.len());
        for line in list.iter().take(5) {
            println!("   {line}");
        }
    }
    let total = |doc: &GaebDocument| {
        doc.award
            .boq
            .totals
            .as_ref()
            .and_then(|totals| totals.total)
    };
    println!(
        "LV-Summe in der Datei: {:?} / {:?}; gerechnet: {} / {}",
        total(&left),
        total(&right),
        xml::computed_total(&left.award.boq.body, &[]),
        xml::computed_total(&right.award.boq.body, &[])
    );
}
