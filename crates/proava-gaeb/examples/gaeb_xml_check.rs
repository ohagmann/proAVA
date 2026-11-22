//! Reads all GAEB DA XML files (X81–X86) below the folders in
//! `PROAVA_GAEB_TESTDATA` (several separated by ':') and prints an overview:
//! phase, titles, items, total computed from quantity × unit price, total in
//! the file and the difference. Each file is also written again in its phase
//! and read back; "RT ok" means the same model came out.
//!
//! The files are only read; nothing is copied anywhere. With
//! `PROAVA_GAEB_WRITE_DIR` every file is also written as X81, X82, X83 and
//! X84 into that folder (for the XSD check, see `scripts/validate_xsd.py`).
//!
//! ```sh
//! PROAVA_GAEB_TESTDATA=/path/to/lvs cargo run -p proava-gaeb --example gaeb_xml_check
//! ```

use std::path::{Path, PathBuf};

use proava_gaeb::xml;
use proava_gaeb::{BoqNode, GaebDocument, Phase};

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension.len() == 3
            && extension.starts_with("x8")
            && matches!(&extension[2..], "1" | "2" | "3" | "4" | "5" | "6")
        {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for entry in entries {
        collect(&entry, out);
    }
}

#[derive(Default)]
struct Counts {
    categories: usize,
    items: usize,
    remarks: usize,
    item_total_mismatch: usize,
    category_total_mismatch: Vec<String>,
}

fn count(nodes: &[BoqNode], path: &mut Vec<String>, counts: &mut Counts) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                counts.categories += 1;
                path.push(category.rno_part.clone());
                count(&category.body, path, counts);
                if let Some(given) = category.totals.as_ref().and_then(|totals| totals.total) {
                    let computed = xml::computed_total(&category.body, path);
                    if computed != given {
                        counts.category_total_mismatch.push(format!(
                            "{} ({} ≠ {})",
                            path.join("."),
                            computed,
                            given
                        ));
                    }
                }
                path.pop();
            }
            BoqNode::Item(item) => {
                counts.items += 1;
                if let (Some(given), Some(computed)) = (item.total, xml::item_total(item)) {
                    if given != computed {
                        counts.item_total_mismatch += 1;
                    }
                }
            }
            BoqNode::Remark(_) | BoqNode::PerfDescr(_) => counts.remarks += 1,
        }
    }
}

fn window(left: &str, right: &str) -> Option<String> {
    let position = left
        .char_indices()
        .zip(right.chars())
        .find(|((_, a), b)| a != b)
        .map(|((index, _), _)| index)
        .or_else(|| (left.len() != right.len()).then(|| left.len().min(right.len())))?;
    let start = left[..position]
        .char_indices()
        .rev()
        .nth(120)
        .map(|(index, _)| index)
        .unwrap_or(0);
    let cut = |text: &str| {
        text.get(start..)
            .unwrap_or("")
            .chars()
            .take(320)
            .collect::<String>()
    };
    Some(format!("…{}\n   ≠ …{}", cut(left), cut(right)))
}

fn first_difference(a: &GaebDocument, b: &GaebDocument) -> Option<String> {
    if a.project != b.project {
        return window(&format!("{:?}", a.project), &format!("{:?}", b.project))
            .map(|diff| format!("Projekt: {diff}"));
    }
    let mut award_a = a.award.clone();
    let mut award_b = b.award.clone();
    award_a.boq.body.clear();
    award_b.boq.body.clear();
    if award_a != award_b {
        return Some(format!(
            "Kopf: {}",
            window(&format!("{award_a:?}"), &format!("{award_b:?}")).unwrap_or_default()
        ));
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    flatten_nodes(&a.award.boq.body, &mut left);
    flatten_nodes(&b.award.boq.body, &mut right);
    for (index, (x, y)) in left.iter().zip(&right).enumerate() {
        if x != y {
            return Some(format!(
                "Knoten {index}: {}",
                window(&format!("{x:?}"), &format!("{y:?}")).unwrap_or_default()
            ));
        }
    }
    (left.len() != right.len()).then(|| format!("Knotenzahl {} ≠ {}", left.len(), right.len()))
}

/// Nodes in document order; titles without their body.
fn flatten_nodes(nodes: &[BoqNode], out: &mut Vec<BoqNode>) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                let mut head = category.clone();
                head.body.clear();
                out.push(BoqNode::Category(head));
                flatten_nodes(&category.body, out);
            }
            other => out.push(other.clone()),
        }
    }
}

/// Ordinal with leading zeros removed per piece: some bidder programs write
/// "1.489.20.10" for "01.489.20.0010".
fn key(ordinal: &str) -> String {
    ordinal
        .split('.')
        .map(|piece| {
            let trimmed = piece.trim_start_matches('0');
            if trimmed.is_empty() {
                "0"
            } else {
                trimmed
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Takes quantities, item kinds and surcharge references of an X84 from the
/// matching tender (most common ordinals, nearest folder) – as the
/// application does when it imports a bid.
fn merge_with_tender(
    bid_path: &Path,
    bid: &mut GaebDocument,
    tenders: &[(PathBuf, GaebDocument)],
) -> Option<String> {
    let bid_items: Vec<String> = bid
        .award
        .boq
        .items()
        .iter()
        .map(|(pieces, _)| key(&pieces.join(".")))
        .collect();
    let mut best: Option<((usize, usize), &PathBuf, &GaebDocument)> = None;
    for (path, tender) in tenders {
        let ordinals: std::collections::HashSet<String> = tender
            .award
            .boq
            .items()
            .iter()
            .map(|(pieces, _)| key(&pieces.join(".")))
            .collect();
        let common = bid_items
            .iter()
            .filter(|ordinal| ordinals.contains(*ordinal))
            .count();
        let nearness = path
            .components()
            .zip(bid_path.components())
            .take_while(|(a, b)| a == b)
            .count();
        let score = (nearness, common);
        if common * 10 >= bid_items.len() * 9
            && best.is_none_or(|(best_score, _, _)| score > best_score)
        {
            best = Some((score, path, tender));
        }
    }
    let (_, path, tender) = best?;
    let source: std::collections::HashMap<String, proava_gaeb::Item> = tender
        .award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, item)| (key(&pieces.join(".")), item.clone()))
        .collect();
    fn apply(
        nodes: &mut [BoqNode],
        path: &mut Vec<String>,
        source: &std::collections::HashMap<String, proava_gaeb::Item>,
    ) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    apply(&mut category.body, path, source);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    if let Some(tender) = source.get(&key(&xml::ordinal(path, item))) {
                        if item.qty.is_none() {
                            item.qty = tender.qty;
                        }
                        item.kind = tender.kind.clone();
                        item.not_applicable = tender.not_applicable;
                        if let (Some(own), Some(theirs)) = (&mut item.surcharge, &tender.surcharge)
                        {
                            own.markup_type = theirs.markup_type.clone();
                            own.items = theirs.items.clone();
                        }
                    }
                }
                _ => {}
            }
        }
    }
    apply(&mut bid.award.boq.body, &mut Vec::new(), &source);
    Some(
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    )
}

fn main() {
    let Ok(roots) = std::env::var("PROAVA_GAEB_TESTDATA") else {
        eprintln!(
            "PROAVA_GAEB_TESTDATA ist nicht gesetzt (Ordner mit GAEB-Dateien, mehrere mit ':')."
        );
        std::process::exit(2);
    };
    let verbose = std::env::var("PROAVA_GAEB_VERBOSE").is_ok();
    let mut files = Vec::new();
    for root in roots.split(':').filter(|root| !root.is_empty()) {
        collect(Path::new(root), &mut files);
    }
    println!("{} Dateien", files.len());
    let mut failures = 0;
    let mut roundtrip_failures = 0;
    let mut loaded: Vec<(PathBuf, GaebDocument, xml::XmlReadReport)> = Vec::new();
    for file in &files {
        let bytes = match std::fs::read(file) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("FEHLER {}: {error}", file.display());
                failures += 1;
                continue;
            }
        };
        match xml::read_with_report(&bytes) {
            Ok((doc, report)) => loaded.push((file.clone(), doc, report)),
            Err(error) => {
                println!("FEHLER {}: {error}", file.display());
                failures += 1;
            }
        }
    }
    let tenders: Vec<(PathBuf, GaebDocument)> = loaded
        .iter()
        .filter(|(_, doc, _)| matches!(doc.phase, Phase::Dp81 | Phase::Dp82 | Phase::Dp83))
        .map(|(path, doc, _)| (path.clone(), doc.clone()))
        .collect();
    let write_dir = std::env::var("PROAVA_GAEB_WRITE_DIR")
        .ok()
        .map(PathBuf::from);
    for (index, (file, doc, report)) in loaded.iter().enumerate() {
        if let Some(dir) = &write_dir {
            let stem = file.file_stem().unwrap_or_default().to_string_lossy();
            for phase in [Phase::Dp81, Phase::Dp82, Phase::Dp83, Phase::Dp84] {
                let target = dir.join(format!("{index:03}_{stem}.{}", xml::file_extension(phase)));
                if let Err(error) = xml::write_file(doc, phase, &target) {
                    println!("SCHREIBFEHLER {}: {error}", target.display());
                }
            }
        }
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let phase = doc.phase;
        let mut checked = doc.clone();
        let partner = if phase == Phase::Dp84 {
            merge_with_tender(file, &mut checked, &tenders)
        } else {
            None
        };
        let mut counts = Counts::default();
        count(&checked.award.boq.body, &mut Vec::new(), &mut counts);
        let computed = xml::computed_total(&checked.award.boq.body, &[]);
        let given = doc
            .award
            .boq
            .totals
            .as_ref()
            .and_then(|totals| totals.total);
        let difference = given.map(|given| computed - given);
        let expected = xml::as_written(doc, phase);
        let roundtrip = match xml::write(doc, phase).and_then(|text| xml::read(text.as_bytes())) {
            Ok(again) => match first_difference(&expected, &again) {
                None => "RT ok".to_string(),
                Some(diff) => {
                    roundtrip_failures += 1;
                    if verbose {
                        println!("   Roundtrip-Unterschied {name}: {diff}");
                    }
                    "RT DIFF".to_string()
                }
            },
            Err(error) => {
                roundtrip_failures += 1;
                format!("RT FEHLER {error}")
            }
        };
        println!(
            "X{} v{:<3} T{:>4} P{:>5} H{:>4} | gerechnet {:>14} | Datei {:>14} | Diff {:>10} | IT≠ {:>3} | Titel≠ {:>3} | W {:>2} | {} | {}{}",
            phase.number(),
            report.version,
            counts.categories,
            counts.items,
            counts.remarks,
            if phase.has_prices() { computed.to_string() } else { "-".into() },
            given.map(|value| value.to_string()).unwrap_or_else(|| "-".into()),
            difference.map(|value| value.to_string()).unwrap_or_else(|| "-".into()),
            counts.item_total_mismatch,
            counts.category_total_mismatch.len(),
            report.warnings.len(),
            roundtrip,
            file.display(),
            partner.map(|partner| format!("  [Mengen/Arten aus {partner}]")).unwrap_or_default()
        );
        if verbose {
            for warning in report.warnings.iter().take(5) {
                println!("   Warnung: {warning}");
            }
            for title in counts.category_total_mismatch.iter().take(5) {
                println!("   Titel≠ {title}");
            }
        }
    }
    println!(
        "Ergebnis: {} Dateien, {} Lesefehler, {} Roundtrip-Abweichungen",
        files.len(),
        failures,
        roundtrip_failures
    );
}
