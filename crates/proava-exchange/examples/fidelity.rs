//! Fidelity check "GAEB → project → file → project → GAEB".
//!
//! For every LV file (X81/X82/X83/X86, D81/D82/D83/D86; bids 84/85 are left
//! out) below the folders in `PROAVA_GAEB_TESTDATA` (several separated by
//! `:`): import it as a project, save it as `.proava`, load it again,
//! export it in its own format and phase (86 as 82) and read the export.
//! The re-read document is compared with the original one: outline and
//! order, texts (whitespace collapsed, formatting counted), quantities,
//! units, prices, item kinds, references, text complements, hints,
//! performance descriptions, preliminary texts, project data and totals.
//!
//! Each finding is also checked against a direct codec round trip (write
//! the original document in the target phase and read it back); findings
//! that appear there too are marked "Codec" – they are not caused by the
//! project model.
//!
//! Folders named "Alte Versionen"/"Alte Vrsionen" are skipped. The report
//! goes to `PROAVA_FIDELITY_REPORT` (default
//! `/tmp/proava/fidelity/bericht.md`), work files to
//! `PROAVA_FIDELITY_WORK` (default `/tmp/proava/fidelity/work`). Test data
//! never enters the repository.
//!
//! ```sh
//! PROAVA_GAEB_TESTDATA=/path/a:/path/b \
//!     cargo run -p proava-exchange --example fidelity --release
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use proava_application::{ExchangeFiles, GaebExport, ProjectRepository};
use proava_domain::{Decimal, ProjectName};
use proava_exchange::GaebPdfExchange;
use proava_gaeb::{
    datan, xml, BoqNode, BreakdownKind, GaebDocument, Item, Phase, Provisional, RichText, TextBlock,
};
use proava_infrastructure::XmlProjectFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Xml,
    Datan,
}

impl Format {
    fn letter(self) -> char {
        match self {
            Format::Xml => 'X',
            Format::Datan => 'D',
        }
    }
}

/// One difference between the original and the round-tripped document.
#[derive(Debug, Clone)]
struct Finding {
    kind: String,
    oz: String,
    before: String,
    after: String,
    codec: bool,
}

impl Finding {
    fn key(&self) -> (String, String) {
        (self.kind.clone(), self.oz.clone())
    }
}

struct FileResult {
    shown: String,
    format: Format,
    phase: u8,
    target: u8,
    items: usize,
    warnings: Vec<String>,
    outcome: Result<Vec<Finding>, String>,
}

fn main() {
    let Ok(roots) = std::env::var("PROAVA_GAEB_TESTDATA") else {
        eprintln!("PROAVA_GAEB_TESTDATA ist nicht gesetzt (Ordner, mehrere mit ':')");
        std::process::exit(2);
    };
    let report_path = PathBuf::from(
        std::env::var("PROAVA_FIDELITY_REPORT")
            .unwrap_or_else(|_| "/tmp/proava/fidelity/bericht.md".to_string()),
    );
    let work = PathBuf::from(
        std::env::var("PROAVA_FIDELITY_WORK")
            .unwrap_or_else(|_| "/tmp/proava/fidelity/work".to_string()),
    );
    std::fs::create_dir_all(&work).expect("work folder");

    let mut files = Vec::new();
    for root in roots.split(':').filter(|root| !root.trim().is_empty()) {
        collect(Path::new(root), &mut files);
    }
    files.sort();
    files.dedup();

    let mut results = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let shown = shown_name(file, &roots);
        eprintln!("[{}/{}] {shown}", index + 1, files.len());
        results.push(check(file, shown, &work, index));
    }

    let report = render(&results);
    if let Some(parent) = report_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&report_path, &report).expect("write report");
    let faithful = results
        .iter()
        .filter(|result| matches!(&result.outcome, Ok(findings) if findings.is_empty()))
        .count();
    let errors = results
        .iter()
        .filter(|result| result.outcome.is_err())
        .count();
    println!(
        "{} Dateien: {faithful} treu, {} mit Abweichungen, {errors} Fehler. Bericht: {}",
        results.len(),
        results.len() - faithful - errors,
        report_path.display()
    );
}

// ---------- files ----------

fn collect(folder: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if path.is_dir() {
            if name.contains("alte versionen") || name.contains("alte vrsionen") {
                continue;
            }
            collect(&path, out);
        } else if lv_file(&path).is_some() {
            out.push(path);
        }
    }
}

/// Format and phase of an LV file by its extension; bids are not LVs.
fn lv_file(path: &Path) -> Option<(Format, u8)> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let format = match extension.chars().next()? {
        'x' => Format::Xml,
        'd' => Format::Datan,
        _ => return None,
    };
    let phase: u8 = extension.get(1..)?.parse().ok()?;
    matches!(phase, 81 | 82 | 83 | 86).then_some((format, phase))
}

fn shown_name(file: &Path, roots: &str) -> String {
    let text = file.display().to_string();
    for root in roots.split(':') {
        if let Some(rest) = text.strip_prefix(root.trim_end_matches('/')) {
            return rest.trim_start_matches('/').to_string();
        }
    }
    text
}

fn read(format: Format, bytes: &[u8]) -> Result<GaebDocument, String> {
    match format {
        Format::Xml => xml::read(bytes).map_err(|error| error.to_string()),
        Format::Datan => datan::read(bytes)
            .map(|report| report.document)
            .map_err(|error| error.to_string()),
    }
}

fn write(format: Format, doc: &GaebDocument) -> Result<Vec<u8>, String> {
    match format {
        Format::Xml => xml::write(doc, doc.phase)
            .map(String::into_bytes)
            .map_err(|error| error.to_string()),
        Format::Datan => datan::write(doc).map_err(|error| error.to_string()),
    }
}

fn check(file: &Path, shown: String, work: &Path, index: usize) -> FileResult {
    let (format, phase) = lv_file(file).expect("filtered");
    let target = if phase == 86 { 82 } else { phase };
    let mut result = FileResult {
        shown,
        format,
        phase,
        target,
        items: 0,
        warnings: Vec::new(),
        outcome: Err(String::new()),
    };
    result.outcome = (|| {
        let bytes = std::fs::read(file).map_err(|error| error.to_string())?;
        let original = read(format, &bytes).map_err(|error| format!("Lesen: {error}"))?;
        result.items = original.award.boq.items().len();

        // Through the project, exactly as the program does it.
        let exchange = GaebPdfExchange;
        let name = ProjectName::parse("Treue").expect("valid name");
        let (project, warnings) = exchange
            .import_lv(file, &name, "2026-11-17T18:00:00")
            .map_err(|error| format!("Import: {error}"))?;
        result.warnings = warnings;
        let project_file = work.join(format!("{index:04}.proava"));
        let repository = XmlProjectFile::new();
        repository
            .save_project(&project_file, &project)
            .map_err(|error| format!("Speichern: {error}"))?;
        let loaded = repository
            .load_project(&project_file)
            .map_err(|error| format!("Laden: {error}"))?;
        let kind = match target {
            81 => GaebExport::Description,
            83 => GaebExport::RequestForQuotation,
            _ => GaebExport::PricedLv,
        };
        let export_file = work.join(format!("{index:04}.{}{target}", format.letter()));
        exchange
            .export_gaeb(&loaded, kind, &export_file)
            .map_err(|error| format!("Export: {error}"))?;
        let exported = std::fs::read(&export_file).map_err(|error| error.to_string())?;
        let reread = read(format, &exported).map_err(|error| format!("Export lesen: {error}"))?;
        let mut findings = compare(&original, &reread);

        // Direct codec round trip to tell codec losses apart.
        let mut direct = original.clone();
        direct.phase = Phase::from_number(target).expect("known phase");
        let codec: BTreeSet<(String, String)> =
            match write(format, &direct).and_then(|bytes| read(format, &bytes)) {
                Ok(back) => compare(&original, &back).iter().map(Finding::key).collect(),
                Err(_) => BTreeSet::new(),
            };
        for finding in &mut findings {
            finding.codec = codec.contains(&finding.key());
        }
        Ok(findings)
    })();
    result
}

// ---------- flat view of a document ----------

fn norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn plain(text: &RichText) -> String {
    let mut parts = Vec::new();
    for block in &text.blocks {
        match block {
            TextBlock::Paragraph { runs, .. } | TextBlock::ListItem { runs, .. } => {
                parts.push(runs.iter().map(|run| run.text.as_str()).collect::<String>())
            }
            TextBlock::Table(rows) => {
                for cells in rows.texts() {
                    parts.push(cells.join(" "));
                }
            }
            TextBlock::Image { .. } => {}
        }
    }
    norm(&parts.join(" "))
}

fn formatting(text: &RichText) -> String {
    let (mut bold, mut italic, mut underline) = (0, 0, 0);
    let (mut bullets, mut numbered, mut tables, mut images) = (0, 0, 0, 0);
    for block in &text.blocks {
        let runs = match block {
            TextBlock::Paragraph { runs, .. } => runs,
            TextBlock::ListItem { ordered, runs, .. } => {
                if *ordered {
                    numbered += 1;
                } else {
                    bullets += 1;
                }
                runs
            }
            TextBlock::Table(_) => {
                tables += 1;
                continue;
            }
            TextBlock::Image { .. } => {
                images += 1;
                continue;
            }
        };
        for run in runs {
            let count = run.text.chars().filter(|ch| !ch.is_whitespace()).count();
            if run.bold {
                bold += count;
            }
            if run.italic {
                italic += count;
            }
            if run.underline {
                underline += count;
            }
        }
    }
    let mut parts = Vec::new();
    for (label, count) in [
        ("fett", bold),
        ("kursiv", italic),
        ("unterstrichen", underline),
        ("Aufzählung", bullets),
        ("Nummerierung", numbered),
        ("Tabelle", tables),
        ("Bild", images),
    ] {
        if count > 0 {
            parts.push(format!("{label} {count}"));
        }
    }
    parts.join(", ")
}

fn number(value: Option<Decimal>) -> String {
    value
        .map(|value| value.normalize().to_string())
        .unwrap_or_default()
}

fn flag(set: bool) -> String {
    if set { "ja" } else { "" }.to_string()
}

fn trimmed_number(text: &str) -> String {
    let text = text.trim();
    let stripped = text.trim_start_matches('0');
    if stripped.is_empty() && !text.is_empty() {
        "0".to_string()
    } else {
        stripped.to_string()
    }
}

/// Fields compared only when both documents carry prices.
const PRICED: [&str; 5] = ["EP", "EP-Anteile", "Zeitansatz", "Zuschlag %", "MwSt"];

fn item_fields(item: &Item) -> Vec<(&'static str, String)> {
    let kind = &item.kind;
    let surcharge =
        item.surcharge.is_some() || item.markup_percent.is_some() || item.markup_base.is_some();
    let art = match (&kind.alternative, kind.provisional) {
        (Some(alternative), _) if trimmed_number(&alternative.serial) == "0" => {
            format!("Grund {}", trimmed_number(&alternative.group))
        }
        (Some(alternative), _) => format!(
            "Wahl {}/{}",
            trimmed_number(&alternative.group),
            trimmed_number(&alternative.serial)
        ),
        (None, Some(Provisional::WithTotal)) => "Bedarf mit GB".to_string(),
        (None, Some(Provisional::WithoutTotal)) => "Bedarf ohne GB".to_string(),
        (None, None) => String::new(),
    };
    let surcharge_text = if surcharge {
        let mut text = item
            .surcharge
            .as_ref()
            .map(|surcharge| surcharge.markup_type.clone())
            .unwrap_or_default();
        if text.is_empty() {
            text = "IdentAsMark".to_string();
        }
        if let Some(surcharge) = &item.surcharge {
            if !surcharge.items.is_empty() {
                let list: Vec<String> = surcharge
                    .items
                    .iter()
                    .map(|entry| format!("{} {}", entry.ordinal, number(entry.sub_qty)))
                    .collect();
                text.push_str(&format!(" [{}]", list.join("; ")));
            }
        }
        text
    } else {
        String::new()
    };
    let complements: Vec<String> = item
        .complements
        .iter()
        .map(|complement| {
            norm(&format!(
                "{}|{}|{}|{}|{}",
                complement.marker.trim(),
                complement.kind,
                complement.caption,
                complement.body,
                complement.tail
            ))
        })
        .collect();
    let subs: Vec<String> = item
        .sub_descriptions
        .iter()
        .map(|sub| {
            norm(&format!(
                "{} {} {} {} | {}",
                trimmed_number(&sub.number),
                number(sub.qty),
                sub.unit,
                sub.short_text,
                plain(&sub.long_text)
            ))
        })
        .collect();
    let components: Vec<String> = item.up_components.iter().map(|c| number(*c)).collect();
    vec![
        ("Kurztext", norm(&item.short_text)),
        ("Langtext", plain(&item.long_text)),
        ("Formatierung", formatting(&item.long_text)),
        // A missing quantity and 0 mean the same to the user.
        ("Menge", number(Some(item.qty.unwrap_or(Decimal::ZERO)))),
        ("Menge offen (QtyTBD)", flag(item.qty_to_be_determined)),
        ("Einheit", item.unit.trim().to_string()),
        ("EP", number(item.unit_price)),
        (
            "EP-Anteile",
            components.join("; ").trim_matches([';', ' ']).to_string(),
        ),
        ("Zeitansatz", number(item.time_quantity)),
        ("Positionsart", art),
        ("Pauschal", flag(kind.lump_sum)),
        ("Stundenlohn", flag(kind.hourly)),
        ("Zuschlagsposition", surcharge_text),
        ("Zuschlag %", number(item.markup_percent)),
        ("Zuschlagsfähig (MarkupIt)", flag(kind.markup && !surcharge)),
        (
            "Bezug Ausführungsbeschreibung",
            kind.reference_description
                .as_deref()
                .map(trimmed_number)
                .unwrap_or_default(),
        ),
        (
            "Wiederholung von",
            kind.repetition_of.clone().unwrap_or_default(),
        ),
        ("Textergänzungen", complements.join(" ¦ ")),
        ("Unterbeschreibungen", subs.join(" ¦ ")),
        ("entfällt (NotAppl)", flag(item.not_applicable)),
        ("Schwerpunkt (KeyIt)", flag(item.key_item)),
        ("EP-Aufgliederung (UPBkdn)", flag(item.up_breakdown)),
    ]
}

#[derive(Clone)]
struct RemarkView {
    anchor: String,
    heading: String,
    body: String,
}

#[derive(Default)]
struct Flat {
    head: Vec<(&'static str, String)>,
    outline: Vec<String>,
    categories: Vec<(String, Vec<(&'static str, String)>)>,
    items: Vec<(String, Vec<(&'static str, String)>)>,
    remarks: Vec<RemarkView>,
    perfs: Vec<(String, String, String)>,
    add_texts: Vec<String>,
    contract: String,
}

fn flatten(doc: &GaebDocument) -> Flat {
    let mut flat = Flat::default();
    let award = &doc.award;
    let boq = &award.boq;
    let owner = award
        .owner
        .as_ref()
        .map(|owner| {
            norm(
                &[
                    &owner.name1,
                    &owner.name2,
                    &owner.street,
                    &owner.postal_code,
                    &owner.city,
                    &owner.phone,
                    &owner.fax,
                    &owner.email,
                    &owner.contact,
                ]
                .iter()
                .map(|part| part.trim())
                .collect::<Vec<_>>()
                .join(" | "),
            )
        })
        .unwrap_or_default();
    let mask: Vec<String> = boq
        .breakdown
        .iter()
        .map(|level| {
            let kind = match level.kind {
                BreakdownKind::Lot => "Los",
                BreakdownKind::Level => "Stufe",
                BreakdownKind::Item => "Pos",
                BreakdownKind::Index => "Index",
            };
            format!(
                "{kind}{}{}",
                level.length,
                if level.numeric { "" } else { "a" }
            )
        })
        .collect();
    let submission = &award.submission;
    flat.head = vec![
        ("Projektname (NamePrj)", norm(&doc.project.name)),
        ("Projektbezeichnung (LblPrj)", norm(&doc.project.label)),
        ("LV-Name (BoQ/Name)", norm(&boq.name)),
        ("LV-Bezeichnung (LblBoQ)", norm(&boq.label)),
        ("Auftraggeber", owner),
        ("Vergabenummer", norm(&award.award_no)),
        (
            "Eröffnung/Abgabeort",
            norm(&format!(
                "{} {} {}",
                submission.location,
                submission.open_date.clone().unwrap_or_default(),
                submission.open_time.clone().unwrap_or_default()
            )),
        ),
        (
            "Ausführungszeit (CnstStart/End)",
            norm(&format!(
                "{} {}",
                submission.construction_start.clone().unwrap_or_default(),
                submission.construction_end.clone().unwrap_or_default()
            )),
        ),
        (
            "Bindefrist (EvalEnd/DATZUS)",
            submission.award_deadline.clone().unwrap_or_default(),
        ),
        ("Vergabeart (Cat)", award.category.trim().to_string()),
        ("Währung", doc.project.currency.trim().to_string()),
        ("OZ-Maske", mask.join(".")),
        (
            "MwSt",
            number(boq.totals.as_ref().and_then(|totals| totals.vat_percent)),
        ),
    ];
    flat.contract = norm(&award.contract_text.join(" "));
    flat.add_texts = award
        .add_texts
        .iter()
        .map(|text| norm(&format!("{} {}", text.outline, plain(&text.detail))))
        .collect();
    walk(
        &boq.body,
        &mut Vec::new(),
        &mut flat,
        &mut "Anfang".to_string(),
    );
    flat
}

fn walk(nodes: &[BoqNode], path: &mut Vec<String>, flat: &mut Flat, anchor: &mut String) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                path.push(category.rno_part.trim().to_string());
                let oz = path.join(".");
                flat.outline.push(format!("T {oz}"));
                let alternative = category
                    .alternative
                    .as_ref()
                    .map(|alternative| {
                        format!(
                            "{}/{}",
                            trimmed_number(&alternative.group),
                            trimmed_number(&alternative.serial)
                        )
                    })
                    .unwrap_or_default();
                flat.categories.push((
                    oz.clone(),
                    vec![
                        ("Titeltext", norm(&category.label)),
                        ("Wahl-/Grundbereich (Titel)", alternative),
                    ],
                ));
                *anchor = format!("Titel {oz}");
                walk(&category.body, path, flat, anchor);
                path.pop();
            }
            BoqNode::Item(item) => {
                let mut pieces = path.clone();
                pieces.push(item.rno_part.trim().to_string());
                if !item.rno_index.trim().is_empty() {
                    pieces.push(item.rno_index.trim().to_string());
                }
                let oz = pieces.join(".");
                flat.outline.push(format!("P {oz}"));
                flat.items.push((oz.clone(), item_fields(item)));
                *anchor = format!("nach {oz}");
            }
            BoqNode::Remark(remark) => flat.remarks.push(RemarkView {
                anchor: anchor.clone(),
                heading: norm(&remark.short_text),
                body: plain(&remark.text),
            }),
            BoqNode::PerfDescr(perf) => flat.perfs.push((
                trimmed_number(&perf.number),
                norm(&format!("{} {}", perf.label, perf.short_text)),
                plain(&perf.long_text),
            )),
        }
    }
}

// ---------- comparison ----------

fn compare(original: &GaebDocument, reread: &GaebDocument) -> Vec<Finding> {
    let priced = original.phase.has_prices() && reread.phase.has_prices();
    let a = flatten(original);
    let b = flatten(reread);
    let mut out = Vec::new();
    let mut push = |kind: &str, oz: &str, before: String, after: String| {
        out.push(Finding {
            kind: kind.to_string(),
            oz: oz.to_string(),
            before,
            after,
            codec: false,
        });
    };

    for ((field, before), (_, after)) in a.head.iter().zip(&b.head) {
        if before != after && (priced || !PRICED.contains(field)) {
            push(field, "Kopf", before.clone(), after.clone());
        }
    }
    if priced {
        let totals = |doc: &GaebDocument| {
            doc.award
                .boq
                .totals
                .as_ref()
                .and_then(|totals| totals.total)
                .map(|total| total.round_dp(2))
        };
        if let (Some(before), after) = (totals(original), totals(reread)) {
            if Some(before) != after {
                push("Summe", "LV", number(Some(before)), number(after));
            }
        }
    }

    // Outline: missing, additional, order of the common entries.
    let a_set: BTreeSet<&String> = a.outline.iter().collect();
    let b_set: BTreeSet<&String> = b.outline.iter().collect();
    for entry in a.outline.iter().filter(|entry| !b_set.contains(entry)) {
        push(
            "Gliederung: fehlt",
            &entry[2..],
            entry.clone(),
            String::new(),
        );
    }
    for entry in b.outline.iter().filter(|entry| !a_set.contains(entry)) {
        push(
            "Gliederung: zusätzlich",
            &entry[2..],
            String::new(),
            entry.clone(),
        );
    }
    let common_a: Vec<&String> = a.outline.iter().filter(|e| b_set.contains(e)).collect();
    let common_b: Vec<&String> = b.outline.iter().filter(|e| a_set.contains(e)).collect();
    if let Some(index) = common_a
        .iter()
        .zip(&common_b)
        .position(|(left, right)| left != right)
    {
        push(
            "Reihenfolge",
            &common_a[index][2..],
            common_a[index].to_string(),
            common_b[index].to_string(),
        );
    }

    let compare_fields =
        |oz: &str,
         before: &[(&'static str, String)],
         after: &[(&'static str, String)],
         push: &mut dyn FnMut(&str, &str, String, String)| {
            for ((field, left), (_, right)) in before.iter().zip(after) {
                if left != right && (priced || !PRICED.contains(field)) {
                    push(field, oz, left.clone(), right.clone());
                }
            }
        };
    let b_categories: HashMap<&String, &Vec<(&'static str, String)>> = b
        .categories
        .iter()
        .map(|(oz, fields)| (oz, fields))
        .collect();
    for (oz, fields) in &a.categories {
        if let Some(other) = b_categories.get(oz) {
            compare_fields(oz, fields, other, &mut push);
        }
    }
    let mut b_items: HashMap<&String, Vec<&Vec<(&'static str, String)>>> = HashMap::new();
    for (oz, fields) in &b.items {
        b_items.entry(oz).or_default().push(fields);
    }
    for list in b_items.values_mut() {
        list.reverse();
    }
    for (oz, fields) in &a.items {
        if let Some(other) = b_items.get_mut(oz).and_then(|list| list.pop()) {
            compare_fields(oz, fields, other, &mut push);
        }
    }

    // Hints, performance descriptions, preliminary and contract texts.
    let mut unmatched: Vec<Option<RemarkView>> = b.remarks.iter().cloned().map(Some).collect();
    let mut take = |predicate: &dyn Fn(&RemarkView) -> bool| -> Option<RemarkView> {
        unmatched
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(predicate))
            .and_then(Option::take)
    };
    for remark in &a.remarks {
        let found = take(&|other| other.body == remark.body && other.heading == remark.heading)
            .or_else(|| take(&|other| other.body == remark.body));
        match found {
            Some(other) => {
                if other.heading != remark.heading {
                    push(
                        "Hinweis: Überschrift",
                        &remark.anchor,
                        remark.heading.clone(),
                        other.heading.clone(),
                    );
                }
                if other.anchor != remark.anchor {
                    push(
                        "Hinweis: Lage",
                        &remark.anchor,
                        remark.anchor.clone(),
                        other.anchor.clone(),
                    );
                }
            }
            None => {
                let joined = norm(&format!("{} {}", remark.heading, remark.body));
                match take(&|other| {
                    !joined.is_empty()
                        && norm(&format!("{} {}", other.heading, other.body)) == joined
                }) {
                    Some(_) => push(
                        "Hinweis: Überschrift in den Text",
                        &remark.anchor,
                        remark.heading.clone(),
                        String::new(),
                    ),
                    None => push(
                        "Hinweis: fehlt/geändert",
                        &remark.anchor,
                        joined,
                        String::new(),
                    ),
                }
            }
        }
    }
    let b_perfs: BTreeMap<&String, &(String, String, String)> =
        b.perfs.iter().map(|perf| (&perf.0, perf)).collect();
    for perf in &a.perfs {
        match b_perfs.get(&perf.0) {
            Some(other) => {
                if other.1 != perf.1 {
                    push(
                        "Ausführungsbeschreibung: Titel",
                        &perf.0,
                        perf.1.clone(),
                        other.1.clone(),
                    );
                }
                if other.2 != perf.2 {
                    push(
                        "Ausführungsbeschreibung: Text",
                        &perf.0,
                        perf.2.clone(),
                        other.2.clone(),
                    );
                }
            }
            None => {
                let body = perf.2.clone();
                let as_hint = take(&|other| !body.is_empty() && other.body.contains(&body));
                push(
                    if as_hint.is_some() {
                        "Ausführungsbeschreibung: nur als Hinweis"
                    } else {
                        "Ausführungsbeschreibung: fehlt"
                    },
                    &format!("AB {}", perf.0),
                    perf.1.clone(),
                    String::new(),
                );
            }
        }
    }
    let mut b_add: Vec<Option<&String>> = b.add_texts.iter().map(Some).collect();
    for text in &a.add_texts {
        if let Some(slot) = b_add.iter_mut().find(|slot| *slot == &Some(text)) {
            *slot = None;
            continue;
        }
        let as_hint = take(&|other| {
            !text.is_empty() && norm(&format!("{} {}", other.heading, other.body)) == *text
        });
        push(
            if as_hint.is_some() {
                "Vorbemerkung: als Hinweis"
            } else {
                "Vorbemerkung: fehlt/geändert"
            },
            "Kopf",
            text.clone(),
            String::new(),
        );
    }
    if a.contract != b.contract {
        let contract = a.contract.clone();
        let as_hint = take(&|other| !contract.is_empty() && other.body.contains(&contract));
        push(
            if as_hint.is_some() {
                "Vertragstext (T1): als Hinweis"
            } else {
                "Vertragstext (T1)"
            },
            "Kopf",
            a.contract.clone(),
            b.contract.clone(),
        );
    }
    for remark in unmatched.into_iter().flatten() {
        push(
            "Hinweis: zusätzlich",
            &remark.anchor,
            String::new(),
            norm(&format!("{} {}", remark.heading, remark.body)),
        );
    }
    out
}

// ---------- report ----------

fn clip(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// Short excerpts around the first difference of two long values.
fn excerpt(before: &str, after: &str) -> (String, String) {
    if before.chars().count() <= 90 && after.chars().count() <= 90 {
        return (before.to_string(), after.to_string());
    }
    let left: Vec<char> = before.chars().collect();
    let right: Vec<char> = after.chars().collect();
    let first = left
        .iter()
        .zip(&right)
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    let start = first.saturating_sub(30);
    let cut = |chars: &[char]| {
        let from = start.min(chars.len());
        let to = (first + 50).min(chars.len());
        let mut text: String = chars[from..to].iter().collect();
        if from > 0 {
            text.insert(0, '…');
        }
        if to < chars.len() {
            text.push('…');
        }
        text
    };
    (cut(&left), cut(&right))
}

fn md(text: &str) -> String {
    text.replace('|', "\\|").replace('`', "'")
}

fn render(results: &[FileResult]) -> String {
    let mut out = String::new();
    let faithful = results
        .iter()
        .filter(|r| matches!(&r.outcome, Ok(f) if f.is_empty()))
        .count();
    let errors = results.iter().filter(|r| r.outcome.is_err()).count();
    let _ = writeln!(
        out,
        "# Treue-Prüfung GAEB → Projekt → Datei → Projekt → GAEB\n"
    );
    let _ = writeln!(
        out,
        "{} Dateien: **{faithful} treu**, {} mit Abweichungen, {errors} Fehler.\n",
        results.len(),
        results.len() - faithful - errors
    );
    let _ = writeln!(
        out,
        "„Codec“ = dieselbe Abweichung entsteht auch beim direkten Schreiben/Lesen ohne Projekt.\n"
    );

    // Kinds over all files.
    let mut kinds: BTreeMap<String, (BTreeSet<usize>, usize, usize)> = BTreeMap::new();
    for (index, result) in results.iter().enumerate() {
        if let Ok(findings) = &result.outcome {
            for finding in findings {
                let entry = kinds.entry(finding.kind.clone()).or_default();
                entry.0.insert(index);
                entry.1 += 1;
                if finding.codec {
                    entry.2 += 1;
                }
            }
        }
    }
    let mut kinds: Vec<_> = kinds.into_iter().collect();
    kinds.sort_by(|a, b| b.1 .0.len().cmp(&a.1 .0.len()).then(a.0.cmp(&b.0)));
    let _ = writeln!(out, "## Abweichungsarten\n");
    let _ = writeln!(
        out,
        "| Art | Dateien | Fälle | davon Codec |\n|---|---:|---:|---:|"
    );
    for (kind, (files, count, codec)) in &kinds {
        let _ = writeln!(
            out,
            "| {} | {} | {count} | {codec} |",
            md(kind),
            files.len()
        );
    }

    let _ = writeln!(out, "\n## Dateien\n");
    let _ = writeln!(
        out,
        "| Datei | Phase | Pos. | Ergebnis |\n|---|---|---:|---|"
    );
    for result in results {
        let outcome = match &result.outcome {
            Ok(findings) if findings.is_empty() => "treu".to_string(),
            Ok(findings) => {
                let kinds: BTreeSet<&str> = findings.iter().map(|f| f.kind.as_str()).collect();
                format!("{} Abw. ({} Arten)", findings.len(), kinds.len())
            }
            Err(error) => format!("FEHLER: {}", clip(error, 80)),
        };
        let _ = writeln!(
            out,
            "| {} | {}{} → {}{} | {} | {} |",
            md(&result.shown),
            result.format.letter(),
            result.phase,
            result.format.letter(),
            result.target,
            result.items,
            md(&outcome)
        );
    }

    let _ = writeln!(out, "\n## Einzelheiten\n");
    for result in results {
        let Ok(findings) = &result.outcome else {
            continue;
        };
        if findings.is_empty() {
            continue;
        }
        let _ = writeln!(out, "### {}\n", md(&result.shown));
        for warning in &result.warnings {
            let _ = writeln!(out, "> Import: {}", md(&clip(warning, 200)));
        }
        if !result.warnings.is_empty() {
            out.push('\n');
        }
        let mut by_kind: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
        for finding in findings {
            by_kind.entry(&finding.kind).or_default().push(finding);
        }
        for (kind, list) in by_kind {
            let codec = list.iter().filter(|f| f.codec).count();
            let _ = writeln!(
                out,
                "- **{}**: {}{}",
                md(kind),
                list.len(),
                if codec > 0 {
                    format!(" (davon Codec {codec})")
                } else {
                    String::new()
                }
            );
            for finding in list.iter().take(3) {
                let (before, after) = excerpt(&finding.before, &finding.after);
                let _ = writeln!(
                    out,
                    "  - {}: „{}“ → „{}“",
                    md(&finding.oz),
                    md(&clip(&before, 160)),
                    md(&clip(&after, 160))
                );
            }
        }
        out.push('\n');
    }
    out
}
