//! Reads every `*.D81`–`*.D86` below `PROAVA_GAEB_TESTDATA` (several folders
//! separated by `:`) and prints phase, bidder, item count and the computed
//! total against the total in the file. When an `.X8x` of the same name lies
//! next to it, unit prices and totals are compared item by item.
//!
//! Test data stays outside the repository:
//! `PROAVA_GAEB_TESTDATA=/path/to/folder cargo run -p proava-gaeb --example datan_report`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use proava_gaeb::datan;
use proava_gaeb::{BoqNode, Item};
use quick_xml::events::Event;
use rust_decimal::Decimal;

fn main() {
    let Ok(roots) = std::env::var("PROAVA_GAEB_TESTDATA") else {
        eprintln!("PROAVA_GAEB_TESTDATA ist nicht gesetzt");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    for root in roots.split(':').filter(|r| !r.is_empty()) {
        collect(Path::new(root), &mut files);
    }
    files.sort();
    let mut failures = 0;
    for path in &files {
        println!("== {}", path.display());
        match datan::read_file(path) {
            Ok(report) => report_file(path, &report),
            Err(err) => {
                failures += 1;
                println!("   FEHLER: {err}");
            }
        }
    }
    println!("\n{} Dateien, {failures} nicht lesbar", files.len());
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.len() == 3 && e[..2].eq_ignore_ascii_case("d8"))
        {
            out.push(path);
        }
    }
}

/// GAEB 90 sums are sums of GB; a Zuschlagsposition has no GB, so its amount
/// is not part of any sum in the file (unlike GAEB DA XML).
fn item_value(item: &Item) -> Decimal {
    item.total.unwrap_or_default()
}

fn markup_value(item: &Item) -> Decimal {
    match (item.total, item.markup_percent, item.markup_base) {
        (None, Some(p), Some(b)) => (b * p / Decimal::ONE_HUNDRED).round_dp(2),
        _ => Decimal::ZERO,
    }
}

/// Compares category totals of the file with the sums of their content.
fn check_categories(nodes: &[BoqNode], path: &str, mismatches: &mut Vec<String>) -> Decimal {
    let mut sum = Decimal::ZERO;
    for node in nodes {
        match node {
            BoqNode::Category(c) => {
                let here = if path.is_empty() {
                    c.rno_part.clone()
                } else {
                    format!("{path}.{}", c.rno_part)
                };
                let computed = check_categories(&c.body, &here, mismatches);
                let stated = c.totals.as_ref().and_then(|t| t.total);
                if let Some(stated) = stated {
                    if stated != computed {
                        mismatches.push(format!("{here}: Datei {stated}, gerechnet {computed}"));
                    }
                }
                sum += stated.unwrap_or(computed);
            }
            BoqNode::Item(item) => sum += item_value(item),
            _ => {}
        }
    }
    sum
}

fn report_file(path: &Path, report: &datan::ReadReport) {
    let doc = &report.document;
    let boq = &doc.award.boq;
    let items = boq.items();
    let bidder = doc
        .award
        .bidder
        .as_ref()
        .map(|b| b.name1.as_str())
        .unwrap_or("-");
    println!(
        "   Phase {}  Zeichensatz {:?}  Bieter „{bidder}“  Maske {}",
        doc.phase.number(),
        report.encoding,
        boq.breakdown
            .iter()
            .map(|l| format!("{:?}{}", l.kind, l.length))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let texts = items
        .iter()
        .filter(|(_, i)| !i.short_text.is_empty())
        .count();
    let complements: usize = items.iter().map(|(_, i)| i.complements.len()).sum();
    println!(
        "   Positionen {} (Datei: {})  mit Kurztext {texts}  Textergänzungen {complements}",
        items.len(),
        report
            .declared_item_count
            .map_or("-".to_string(), |n| n.to_string())
    );
    let mut mismatches = Vec::new();
    let computed = check_categories(&boq.body, "", &mut mismatches);
    let item_sum: Decimal = items.iter().map(|(_, i)| item_value(i)).sum();
    let markups: Decimal = items.iter().map(|(_, i)| markup_value(i)).sum();
    let totals = report.file_totals.clone().unwrap_or_default();
    let stated = totals.total;
    let verdict = match stated {
        Some(s) if s == item_sum => "OK".to_string(),
        Some(s) => format!("ABWEICHUNG {}", item_sum - s),
        None => "keine Summe in der Datei".to_string(),
    };
    println!(
        "   Summe Positionen {item_sum}  über Gruppen {computed}  Datei {}  -> {verdict}",
        stated.map_or("-".into(), |s| s.to_string())
    );
    if !markups.is_zero() {
        println!("   Zuschläge (Zuschlagspositionen, in GAEB 90 nicht in den Summen): {markups}");
    }
    println!(
        "   Nachlass {}  MwSt {}  Brutto Datei {}",
        totals
            .discount_percent
            .map_or("-".into(), |d| format!("{d} %")),
        totals.vat_percent.map_or("-".into(), |v| format!("{v} %")),
        totals.total_gross.map_or("-".into(), |g| g.to_string()),
    );
    if let (Some(net), Some(vat), Some(gross)) =
        (totals.total, totals.vat_percent, totals.total_gross)
    {
        let after = totals.total_after_discount.unwrap_or(net);
        let expected = (after * (Decimal::ONE_HUNDRED + vat) / Decimal::ONE_HUNDRED).round_dp(2);
        println!(
            "   Brutto gerechnet {expected} (Differenz {})",
            gross - expected
        );
    }
    if mismatches.is_empty() {
        println!("   Gruppensummen: alle passen");
    } else {
        println!("   Gruppensummen abweichend: {}", mismatches.len());
        for m in mismatches.iter().take(5) {
            println!("     {m}");
        }
    }
    println!("   Hinweise beim Lesen: {}", report.warnings.len());
    for w in report.warnings.iter().take(5) {
        println!("     {w}");
    }
    roundtrip(report);
    if let Some(xml) = sibling_xml(path) {
        compare_xml(&xml, &items);
    } else if doc.phase.number() == 84 {
        for x83 in same_folder(path, "x83") {
            compare_structure(&x83, &items);
        }
    }
}

/// Writes the document again as GAEB 90 and reads it back.
fn roundtrip(report: &datan::ReadReport) {
    let doc = &report.document;
    let bytes = match datan::write(doc) {
        Ok(bytes) => bytes,
        Err(err) => {
            println!("   Schreibprobe: FEHLER {err}");
            return;
        }
    };
    let again = match datan::read(&bytes) {
        Ok(again) => again.document,
        Err(err) => {
            println!("   Schreibprobe: neu gelesen FEHLER {err}");
            return;
        }
    };
    let before = doc.award.boq.items();
    let after = again.award.boq.items();
    let same_items = before.len() == after.len()
        && before.iter().zip(&after).all(|(a, b)| {
            a.0 == b.0
                && a.1.unit_price == b.1.unit_price
                && a.1.total == b.1.total
                && a.1.complements == b.1.complements
                && a.1.discount_percent == b.1.discount_percent
        });
    let same_doc = again == *doc;
    println!(
        "   Schreibprobe D{}: {} Zeilen, Positionen/Preise {}, Dokument {}",
        doc.phase.number(),
        bytes.len() / 82,
        if same_items { "gleich" } else { "ABWEICHEND" },
        if same_doc {
            "gleich"
        } else {
            "abweichend (Kurzform ohne Texte/41)"
        }
    );
}

fn sibling_xml(path: &Path) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?.to_lowercase();
    let ext = path.extension()?.to_str()?.to_lowercase();
    let wanted = format!("x{}", &ext[1..]);
    std::fs::read_dir(path.parent()?)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_lowercase)
                == Some(stem.clone())
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase)
                    == Some(wanted.clone())
        })
}

fn same_folder(path: &Path, ext: &str) -> Vec<PathBuf> {
    let Some(Ok(entries)) = path.parent().map(std::fs::read_dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        })
        .collect();
    out.sort();
    out
}

/// Checks that every item of the bid exists in the request (X83).
fn compare_structure(x83: &Path, items: &[(Vec<String>, &Item)]) {
    let name = x83.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let Ok(request) = read_xml_prices(x83) else {
        println!("   {name}: XML nicht lesbar");
        return;
    };
    let found = items
        .iter()
        .filter(|(pieces, _)| request.contains_key(&key(pieces)))
        .count();
    println!(
        "   OZ-Abgleich mit {name}: {found} von {} Positionen gefunden ({} im X83)",
        items.len(),
        request.len()
    );
}

fn key(pieces: &[String]) -> String {
    pieces
        .iter()
        .map(|p| {
            let t = p.trim();
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
                t.trim_start_matches('0').to_string()
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

type Prices = (Option<Decimal>, Option<Decimal>);

fn read_xml_prices(path: &Path) -> Result<BTreeMap<String, Prices>, String> {
    let mut reader = quick_xml::Reader::from_file(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut ctgy: Vec<String> = Vec::new();
    let mut item: Option<(String, Prices)> = None;
    let mut out = BTreeMap::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| e.to_string())?
        {
            Event::Start(e) => {
                let name = e.local_name().as_ref().to_string();
                let rno = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.local_name().as_ref() == "RNoPart")
                    .map(|a| a.value.to_string());
                let index = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.local_name().as_ref() == "RNoIndex")
                    .map(|a| a.value.to_string());
                match name.as_str() {
                    "BoQCtgy" => ctgy.push(rno.unwrap_or_default()),
                    "Item" | "MarkupItem" => {
                        let mut pieces = ctgy.clone();
                        pieces.push(rno.unwrap_or_default());
                        if let Some(i) = index.filter(|i| !i.is_empty()) {
                            pieces.push(i);
                        }
                        item = Some((key(&pieces), (None, None)));
                    }
                    _ => {}
                }
                stack.push(name);
            }
            Event::Text(t) => {
                let in_item = stack.len() >= 2
                    && matches!(stack[stack.len() - 2].as_str(), "Item" | "MarkupItem");
                if let (Some((_, prices)), true) = (item.as_mut(), in_item) {
                    let text = t.as_ref().trim().to_string();
                    let value = text.parse::<Decimal>().ok();
                    match stack.last().map(String::as_str) {
                        Some("UP") => prices.0 = value,
                        Some("IT") => prices.1 = value,
                        _ => {}
                    }
                }
            }
            Event::End(_) => match stack.pop().as_deref() {
                Some("BoQCtgy") => {
                    ctgy.pop();
                }
                Some("Item" | "MarkupItem") => {
                    if let Some((k, p)) = item.take() {
                        out.insert(k, p);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

fn compare_xml(xml: &Path, items: &[(Vec<String>, &Item)]) {
    let name = xml.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let prices = match read_xml_prices(xml) {
        Ok(p) => p,
        Err(err) => {
            println!("   Vergleich mit {name}: XML nicht lesbar ({err})");
            return;
        }
    };
    let mut same = 0;
    let mut differ = Vec::new();
    let mut missing = 0;
    for (pieces, item) in items {
        match prices.get(&key(pieces)) {
            Some((up, it)) if *up == item.unit_price && *it == item.total => same += 1,
            Some((up, it)) => differ.push(format!(
                "{}: D8x EP {:?} GP {:?} / X8x EP {:?} GP {:?}",
                pieces.join("."),
                item.unit_price,
                item.total,
                up,
                it
            )),
            None => missing += 1,
        }
    }
    println!(
        "   Vergleich mit {name}: {} Positionen im XML, gleich {same}, abweichend {}, fehlen im XML {missing}",
        prices.len(),
        differ.len()
    );
    for d in differ.iter().take(5) {
        println!("     {d}");
    }
}
