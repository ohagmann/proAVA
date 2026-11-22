//! Test bench for the GAEB codecs against real files.
//!
//! Finds every GAEB file (`*.x8?`, `*.d8?`, `*.p8?`, any case) below the
//! folders in `PROAVA_GAEB_TESTDATA` (several separated by `:`) and checks
//! each one:
//!
//! - read it (DA XML or GAEB 90 by content), phase, version, titles, items,
//!   computed total against the total in the file, reader warnings;
//! - round trip: write it in its own format and phase, read it back and name
//!   every difference in the model;
//! - DA XML: validate the original and the written file against the GAEB XSD
//!   (`xmllint --noout --schema`, without xmllint through python3-lxml);
//! - cross check X↔D: the same file name in both formats in one folder, or
//!   pairs given in `PROAVA_GAEB_PAIRS` (`a.X83|b.D83;c.X84|d.D84`);
//! - bid against LV: for X84/D84/X85/D85 the LV (81/82/83) in the same or a
//!   parent folder with the best OZ match (more LVs: `PROAVA_GAEB_LV`, paths
//!   separated by ':'); every OZ of the bid must exist, and the total is
//!   recomputed with the item kinds and quantities of the LV;
//! - Fachregeln R1–R13 (`proava_gaeb::rules`) on the original and on the
//!   written file; a finding of the written file that the original already
//!   has counts as taken over, the others as new (caused by proAVA). For
//!   bids also R11 against the LV found.
//!
//! Folders named "Alte Versionen" (also misspelt "Alte Vrsionen") and the
//! output folder of the run itself are skipped. Output: a table on stdout and
//! a Markdown report (`PROAVA_GAEB_REPORT`, default
//! `/tmp/proava/gaeb-muster/bericht.md`). Written files go to
//! `PROAVA_GAEB_OUT` (default `/tmp/proava/gaeb-muster/rt`), the XSDs
//! (3.1, 3.2, 3.3 by their official file names) are searched below
//! `PROAVA_GAEB_XSD_DIR` (default: `proava_gaeb::schemas`, i.e. where
//! `scripts/gaeb_muster_laden.sh` puts them; all subfolders are searched).
//! The test data never enters the repository.
//!
//! ```sh
//! PROAVA_GAEB_TESTDATA=/path/a:/path/b cargo run -p proava-gaeb --example gaeb_check --release
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use proava_gaeb::rules::{self, Finding, Severity};
use proava_gaeb::{datan, xml, BoqNode, GaebDocument, Item, Phase, Provisional};
use rust_decimal::{Decimal, RoundingStrategy};

/// Bridge to the DA XML codec, kept in one place.
mod xml_codec {
    use proava_gaeb::{xml, GaebDocument, Phase};

    pub struct Read {
        pub doc: GaebDocument,
        pub version: String,
        pub warnings: Vec<String>,
    }

    pub fn read(bytes: &[u8]) -> Result<Read, String> {
        let (doc, report) = xml::read_with_report(bytes).map_err(|err| err.to_string())?;
        Ok(Read {
            doc,
            version: report.version,
            warnings: report.warnings,
        })
    }

    pub fn write(doc: &GaebDocument, phase: Phase) -> Result<String, String> {
        xml::write(doc, phase).map_err(|err| err.to_string())
    }

    /// What the writer promises to keep: `read(write(doc)) == as_written(doc)`.
    pub fn as_written(doc: &GaebDocument, phase: Phase) -> GaebDocument {
        xml::as_written(doc, phase)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Xml,
    Datan,
    Gaeb2000,
}

impl Format {
    fn label(self) -> &'static str {
        match self {
            Format::Xml => "XML",
            Format::Datan => "D90",
            Format::Gaeb2000 => "2000",
        }
    }
}

struct Loaded {
    doc: GaebDocument,
    version: String,
    encoding: String,
    warnings: Vec<String>,
    declared_items: Option<u32>,
}

#[derive(Default)]
struct Figures {
    categories: usize,
    items: usize,
    remarks: usize,
    perf_descr: usize,
    /// Total as the codec computes it (`xml::computed_total`).
    computed: Decimal,
    /// Plain sum of the item totals as written (IT/GB), without title discounts.
    it_sum: Decimal,
    file_total: Option<Decimal>,
    /// Items whose IT differs from round(Qty × UP).
    it_mismatch: Vec<String>,
    /// Categories whose stated total differs from the sum of their content.
    category_mismatch: Vec<String>,
}

#[derive(Default)]
struct RoundTrip {
    ok: bool,
    error: Option<String>,
    diffs: Vec<String>,
    note: String,
    written: Option<PathBuf>,
    /// XML only: differences between the original and the re-read file, i.e.
    /// what the writer leaves out or normalises on purpose.
    dropped: Vec<String>,
    /// The written file read again (for R11 against the LV).
    again: Option<GaebDocument>,
    /// Fachregeln of the written file.
    rules: Vec<Finding>,
}

#[derive(Clone, Default)]
struct Schema {
    ok: Option<bool>,
    xsd: String,
    errors: Vec<String>,
}

impl Schema {
    fn cell(&self) -> &'static str {
        match self.ok {
            Some(true) => "ok",
            Some(false) => "FEHLER",
            None => "-",
        }
    }
}

struct Entry {
    path: PathBuf,
    shown: String,
    format: Option<Format>,
    sniffed_version: String,
    sniffed_version_date: String,
    sniffed_phase: Option<u8>,
    loaded: Result<Loaded, String>,
    figures: Figures,
    roundtrip: Option<RoundTrip>,
    schema_original: Schema,
    schema_written: Schema,
    notes: Vec<String>,
    /// Fachregeln of the original file.
    rules: Vec<Finding>,
}

fn main() {
    let Ok(roots) = std::env::var("PROAVA_GAEB_TESTDATA") else {
        eprintln!("PROAVA_GAEB_TESTDATA ist nicht gesetzt (Ordner, mehrere mit ':')");
        std::process::exit(2);
    };
    let roots: Vec<PathBuf> = roots
        .split(':')
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .collect();
    let out_dir = PathBuf::from(
        std::env::var("PROAVA_GAEB_OUT").unwrap_or_else(|_| "/tmp/proava/gaeb-muster/rt".into()),
    );
    let report_path = PathBuf::from(
        std::env::var("PROAVA_GAEB_REPORT")
            .unwrap_or_else(|_| "/tmp/proava/gaeb-muster/bericht.md".into()),
    );
    // Explicit folder, else where scripts/gaeb_muster_laden.sh put them
    // (proava_gaeb::schemas); searched with all subfolders.
    let xsd_root = std::env::var("PROAVA_GAEB_XSD")
        .ok()
        .map(PathBuf::from)
        .or_else(proava_gaeb::schemas::schema_folder)
        .unwrap_or_else(|| PathBuf::from("/tmp/proava/gaeb-muster"));
    let _ = std::fs::create_dir_all(&out_dir);
    let schemas = SchemaIndex::new(&xsd_root);

    let mut files = Vec::new();
    for root in &roots {
        collect(root, &mut files);
    }
    // Files written by an earlier run into the output folder are no test
    // data (a default run writes into /tmp/proava/gaeb-muster/rt).
    let written_before = out_dir.canonicalize().unwrap_or_else(|_| out_dir.clone());
    files.retain(|file| !file.starts_with(&written_before) && !file.starts_with(&out_dir));
    files.sort();
    files.dedup();

    let mut entries: Vec<Entry> = files
        .iter()
        .enumerate()
        .map(|(index, path)| check_file(index + 1, path, &roots, &out_dir))
        .collect();

    validate_all(&mut entries, &schemas);

    let mut cache: HashMap<PathBuf, Option<Loaded>> = HashMap::new();
    let cross = cross_checks(&entries);
    let bids = bid_checks(&mut entries, &mut cache);

    let table = render_table(&entries);
    println!("{table}");
    for (title, lines) in [("X↔D-Vergleich", &cross), ("Angebot↔LV", &bids)] {
        if !lines.is_empty() {
            println!("\n{title}:");
            for line in lines {
                println!("  {line}");
            }
        }
    }
    let markdown = render_markdown(&roots, &entries, &cross, &bids, &table);
    match std::fs::write(&report_path, markdown) {
        Ok(()) => println!("\nBericht: {}", report_path.display()),
        Err(err) => eprintln!("Bericht {} nicht geschrieben: {err}", report_path.display()),
    }
}

// ---------------------------------------------------------------- files ----

fn is_gaeb_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|e| {
            e.len() == 3
                && matches!(&e[..1], "x" | "d" | "p")
                && &e[1..2] == "8"
                && e.as_bytes()[2].is_ascii_digit()
        })
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if is_gaeb_extension(path) {
            out.push(path.to_path_buf());
        }
        return;
    }
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_lowercase)
        .is_some_and(|n| n.contains("alte versionen") || n.contains("alte vrsionen"))
    {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect(&entry.path(), out);
    }
}

fn extension_phase(path: &Path) -> Option<u8> {
    let ext = path.extension()?.to_str()?;
    ext.get(1..3)?.parse().ok()
}

fn shown_name(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(rel) = path.strip_prefix(root) {
            let base = root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            return format!("{base}/{}", rel.display());
        }
    }
    path.display().to_string()
}

fn detect_format(bytes: &[u8]) -> Format {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(0);
    let head = &bytes[start..bytes.len().min(start + 16)];
    if head.starts_with(b"<") {
        Format::Xml
    } else if head.len() >= 6 && head[..6].eq_ignore_ascii_case(b"#begin") {
        Format::Gaeb2000
    } else {
        Format::Datan
    }
}

/// Text of the first `<tag>…</tag>` in the first 64 KiB.
fn sniff_tag(bytes: &[u8], tag: &str) -> Option<String> {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(65536)]);
    let open = format!("<{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find('<')? + start;
    Some(text[start..end].trim().to_string())
}

fn load(bytes: &[u8], format: Format) -> Result<Loaded, String> {
    match format {
        Format::Xml => xml_codec::read(bytes).map(|read| Loaded {
            doc: read.doc,
            version: read.version,
            encoding: "UTF-8".into(),
            warnings: read.warnings,
            declared_items: None,
        }),
        Format::Datan => datan::read(bytes)
            .map(|report| Loaded {
                version: "90".into(),
                encoding: format!("{:?}", report.encoding),
                warnings: report.warnings.iter().map(|w| w.to_string()).collect(),
                declared_items: report.declared_item_count,
                doc: report.document,
            })
            .map_err(|err| err.to_string()),
        Format::Gaeb2000 => Err("GAEB DA 2000 (P8x) wird nicht unterstützt".into()),
    }
}

fn load_path(path: &Path) -> Option<Loaded> {
    let bytes = std::fs::read(path).ok()?;
    load(&bytes, detect_format(&bytes)).ok()
}

fn check_file(index: usize, path: &Path, roots: &[PathBuf], out_dir: &Path) -> Entry {
    let shown = shown_name(path, roots);
    let mut entry = Entry {
        path: path.to_path_buf(),
        shown,
        format: None,
        sniffed_version: String::new(),
        sniffed_version_date: String::new(),
        sniffed_phase: extension_phase(path),
        loaded: Err(String::new()),
        figures: Figures::default(),
        roundtrip: None,
        schema_original: Schema::default(),
        schema_written: Schema::default(),
        notes: Vec::new(),
        rules: Vec::new(),
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            entry.loaded = Err(format!("nicht lesbar: {err}"));
            return entry;
        }
    };
    let format = detect_format(&bytes);
    entry.format = Some(format);
    if format == Format::Xml {
        entry.sniffed_version = sniff_tag(&bytes, "Version").unwrap_or_default();
        entry.sniffed_version_date = sniff_tag(&bytes, "VersDate").unwrap_or_default();
        if let Some(dp) = sniff_tag(&bytes, "DP").and_then(|dp| dp.parse().ok()) {
            entry.sniffed_phase = Some(dp);
        }
    }
    let ext_is_xml = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.to_ascii_lowercase().starts_with('x'));
    if ext_is_xml != (format == Format::Xml) && format != Format::Gaeb2000 {
        entry.notes.push(format!(
            "Endung passt nicht zum Inhalt ({})",
            format.label()
        ));
    }
    entry.loaded = load(&bytes, format);
    if let Ok(loaded) = &entry.loaded {
        entry.rules = file_rules(&loaded.doc, &bytes, format);
        entry.figures = figures(&loaded.doc);
        if let Some(phase) = extension_phase(path) {
            if phase != loaded.doc.phase.number() {
                entry.notes.push(format!(
                    "Endung sagt {phase}, Datei sagt {}",
                    loaded.doc.phase.number()
                ));
            }
        }
        if let Some(declared) = loaded.declared_items {
            if declared as usize != entry.figures.items {
                entry.notes.push(format!(
                    "Satz 99 nennt {declared} Positionen, gelesen {}",
                    entry.figures.items
                ));
            }
        }
        let stem = format!(
            "{index:03}_{}",
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("datei")
                .replace(' ', "_")
        );
        entry.roundtrip = Some(roundtrip(loaded, format, out_dir, &stem));
    }
    entry
}

// -------------------------------------------------------------- figures ----

fn round2(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Whether the item total belongs to the title and LV sums.
fn counts(item: &Item) -> bool {
    let alternative = item
        .kind
        .alternative
        .as_ref()
        .is_some_and(|a| !a.serial.trim().trim_start_matches('0').is_empty());
    !alternative && item.kind.provisional != Some(Provisional::WithoutTotal) && !item.not_applicable
}

fn ordinal(path: &[String], item: &Item) -> String {
    let mut pieces = path.to_vec();
    pieces.push(item.rno_part.clone());
    if !item.rno_index.is_empty() {
        pieces.push(item.rno_index.clone());
    }
    pieces.join(".")
}

fn figures(doc: &GaebDocument) -> Figures {
    let mut figures = Figures {
        file_total: doc.award.boq.totals.as_ref().and_then(|t| t.total),
        ..Figures::default()
    };
    walk_figures(&doc.award.boq.body, &mut Vec::new(), &mut figures);
    figures.computed = xml::computed_total(&doc.award.boq.body, &[]);
    figures
}

fn walk_figures(nodes: &[BoqNode], path: &mut Vec<String>, figures: &mut Figures) {
    for node in nodes {
        match node {
            BoqNode::Category(category) => {
                figures.categories += 1;
                path.push(category.rno_part.clone());
                walk_figures(&category.body, path, figures);
                let inner = xml::computed_total(&category.body, path);
                if let Some(stated) = category.totals.as_ref().and_then(|t| t.total) {
                    if stated != inner {
                        figures.category_mismatch.push(format!(
                            "{}: Datei {stated}, gerechnet {inner}",
                            path.join(".")
                        ));
                    }
                }
                path.pop();
            }
            BoqNode::Item(item) => {
                figures.items += 1;
                if let (Some(qty), Some(up), Some(it)) = (item.qty, item.unit_price, item.total) {
                    let expected = round2(qty * up);
                    if expected != it && item.discount_percent.is_none() {
                        figures.it_mismatch.push(format!(
                            "{}: {qty} × {up} = {expected}, Datei {it}",
                            ordinal(path, item)
                        ));
                    }
                }
                if counts(item) {
                    figures.it_sum += item.total.unwrap_or_default();
                }
            }
            BoqNode::Remark(_) => figures.remarks += 1,
            BoqNode::PerfDescr(_) => figures.perf_descr += 1,
        }
    }
}

// ------------------------------------------------------------ roundtrip ----

fn roundtrip(loaded: &Loaded, format: Format, out_dir: &Path, stem: &str) -> RoundTrip {
    let doc = &loaded.doc;
    let mut result = RoundTrip::default();
    let (bytes, extension) = match format {
        Format::Xml => match xml_codec::write(doc, doc.phase) {
            Ok(text) => (text.into_bytes(), format!("X{}", doc.phase.number())),
            Err(err) => {
                result.error = Some(format!("Schreiben: {err}"));
                return result;
            }
        },
        Format::Datan => match datan::write(doc) {
            Ok(bytes) => (bytes, format!("D{}", doc.phase.number())),
            Err(err) => {
                result.error = Some(format!("Schreiben: {err}"));
                return result;
            }
        },
        Format::Gaeb2000 => return result,
    };
    let written = out_dir.join(format!("{stem}.{extension}"));
    if std::fs::write(&written, &bytes).is_ok() {
        result.written = Some(written);
    }
    let again = match load(&bytes, format) {
        Ok(again) => again.doc,
        Err(err) => {
            result.error = Some(format!("Wieder lesen: {err}"));
            return result;
        }
    };
    result.rules = file_rules(&again, &bytes, format);
    let short_form = format == Format::Datan && matches!(doc.phase, Phase::Dp84 | Phase::Dp85);
    if short_form {
        result.note = "D84/D85 Kurzform: nur OZ, Mengen und Preise verglichen".into();
        result.diffs = diff_documents(&price_view(doc), &price_view(&again));
    } else if format == Format::Xml {
        result.note = "gegen xml::as_written (was der Schreiber zusagt)".into();
        result.diffs = diff_documents(&xml_codec::as_written(doc, doc.phase), &again);
        result.dropped = diff_documents(doc, &again);
    } else {
        result.diffs = diff_documents(doc, &again);
    }
    result.ok = result.diffs.is_empty();
    // Kept for R11 against the LV; other phases need no second copy.
    if matches!(again.phase, Phase::Dp84 | Phase::Dp85) {
        result.again = Some(again);
    }
    result
}

/// Fachregeln of one file: the model in its own phase and the bytes.
fn file_rules(doc: &GaebDocument, bytes: &[u8], format: Format) -> Vec<Finding> {
    let mut findings = rules::check(doc, doc.phase, None);
    findings.extend(match format {
        Format::Xml => rules::check_xml(bytes),
        Format::Datan => rules::check_datan(bytes),
        Format::Gaeb2000 => Vec::new(),
    });
    findings
}

/// Key of a finding to match it between original and written file.
fn finding_key(finding: &Finding) -> (&'static str, String) {
    (
        finding.id,
        finding.ordinal.as_deref().map(oz_key).unwrap_or_default(),
    )
}

/// Findings of the written file that the original does not have.
fn new_findings<'a>(original: &[Finding], written: &'a [Finding]) -> Vec<&'a Finding> {
    let mut left: HashMap<(&'static str, String), usize> = HashMap::new();
    for finding in original {
        *left.entry(finding_key(finding)).or_default() += 1;
    }
    written
        .iter()
        .filter(|finding| match left.get_mut(&finding_key(finding)) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .collect()
}

fn severity_counts<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> [usize; 3] {
    let mut counts = [0; 3];
    for finding in findings {
        counts[finding.severity as usize] += 1;
    }
    counts
}

fn counts_cell(counts: [usize; 3]) -> String {
    if counts == [0; 3] {
        "-".to_string()
    } else {
        format!("{}/{}/{}", counts[0], counts[1], counts[2])
    }
}

impl Entry {
    fn written_rules(&self) -> &[Finding] {
        self.roundtrip
            .as_ref()
            .map(|rt| rt.rules.as_slice())
            .unwrap_or_default()
    }

    fn new_rules(&self) -> Vec<&Finding> {
        new_findings(&self.rules, self.written_rules())
    }
}

/// Copy without the texts that the GAEB 90 short form does not carry.
fn price_view(doc: &GaebDocument) -> GaebDocument {
    fn strip(nodes: &[BoqNode]) -> Vec<BoqNode> {
        nodes
            .iter()
            .filter_map(|node| match node {
                BoqNode::Category(category) => {
                    let mut category = category.clone();
                    category.label.clear();
                    category.body = strip(&category.body);
                    Some(BoqNode::Category(category))
                }
                BoqNode::Item(item) => Some(BoqNode::Item(Item {
                    rno_part: item.rno_part.clone(),
                    rno_index: item.rno_index.clone(),
                    qty: item.qty,
                    unit_price: item.unit_price,
                    up_components: item.up_components.clone(),
                    total: item.total,
                    not_offered: item.not_offered,
                    discount_percent: item.discount_percent,
                    ..Item::default()
                })),
                BoqNode::Remark(_) | BoqNode::PerfDescr(_) => None,
            })
            .collect()
    }
    let mut view = doc.clone();
    view.award.boq.body = strip(&doc.award.boq.body);
    view
}

// ---------------------------------------------------------------- diff ----

/// Names the differences between two documents, at most 40.
fn diff_documents(a: &GaebDocument, b: &GaebDocument) -> Vec<String> {
    let mut out = Vec::new();
    let mut head_a = a.clone();
    let mut head_b = b.clone();
    head_a.award.boq.body.clear();
    head_b.award.boq.body.clear();
    if head_a != head_b {
        for line in debug_diff(&head_a, &head_b, 12) {
            out.push(format!("Kopf {line}"));
        }
    }
    let nodes_a = flatten(&a.award.boq.body);
    let nodes_b = flatten(&b.award.boq.body);
    let keys_a: Vec<&String> = nodes_a.iter().map(|(k, _)| k).collect();
    let keys_b: Vec<&String> = nodes_b.iter().map(|(k, _)| k).collect();
    if keys_a != keys_b {
        let set_a: BTreeSet<&String> = keys_a.iter().copied().collect();
        let set_b: BTreeSet<&String> = keys_b.iter().copied().collect();
        let missing: Vec<&&String> = set_a.difference(&set_b).collect();
        let extra: Vec<&&String> = set_b.difference(&set_a).collect();
        if !missing.is_empty() {
            out.push(format!(
                "fehlen danach ({}): {}",
                missing.len(),
                join_some(missing.iter().map(|s| s.as_str()), 8)
            ));
        }
        if !extra.is_empty() {
            out.push(format!(
                "neu danach ({}): {}",
                extra.len(),
                join_some(extra.iter().map(|s| s.as_str()), 8)
            ));
        }
        if missing.is_empty() && extra.is_empty() {
            out.push("Reihenfolge der Knoten geändert".into());
        }
    }
    let map_b: HashMap<&String, &BoqNode> = nodes_b.iter().map(|(k, n)| (k, n)).collect();
    let mut differing = 0;
    for (key, node_a) in &nodes_a {
        let Some(node_b) = map_b.get(key) else {
            continue;
        };
        if node_a != *node_b {
            differing += 1;
            if out.len() < 40 {
                for line in debug_diff(node_a, *node_b, 4) {
                    out.push(format!("{key} {line}"));
                }
            }
        }
    }
    if differing > 0 {
        out.push(format!("{differing} Knoten unterscheiden sich"));
    }
    out
}

fn join_some<'a>(items: impl Iterator<Item = &'a str>, max: usize) -> String {
    let items: Vec<&str> = items.collect();
    let mut text = items
        .iter()
        .take(max)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > max {
        text.push_str(", …");
    }
    text
}

/// Nodes in document order with a key; categories without their body.
fn flatten(nodes: &[BoqNode]) -> Vec<(String, BoqNode)> {
    fn walk(nodes: &[BoqNode], path: &mut Vec<String>, out: &mut Vec<(String, BoqNode)>) {
        let mut remarks = 0;
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    let mut head = category.clone();
                    head.body.clear();
                    out.push((format!("Titel {}", path.join(".")), BoqNode::Category(head)));
                    walk(&category.body, path, out);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    out.push((format!("OZ {}", ordinal(path, item)), node.clone()));
                }
                BoqNode::Remark(_) => {
                    remarks += 1;
                    out.push((
                        format!("Hinweis {}#{remarks}", path.join(".")),
                        node.clone(),
                    ));
                }
                BoqNode::PerfDescr(perf) => {
                    out.push((
                        format!("AB {}/{}", path.join("."), perf.number),
                        node.clone(),
                    ));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut Vec::new(), &mut out);
    out
}

/// Pretty debug output as (path, value) lines.
fn debug_lines(value: &impl std::fmt::Debug) -> Vec<(String, String)> {
    let text = format!("{value:#?}");
    let mut stack: Vec<String> = Vec::new();
    let mut counters: Vec<usize> = vec![0];
    let mut out = Vec::new();
    for line in text.lines() {
        let depth = (line.len() - line.trim_start().len()) / 4;
        let trimmed = line.trim();
        if trimmed.starts_with('}') || trimmed.starts_with(')') || trimmed.starts_with(']') {
            continue;
        }
        stack.truncate(depth);
        counters.truncate(depth + 1);
        while counters.len() < depth + 1 {
            counters.push(0);
        }
        let (name, rest) = match trimmed.split_once(": ") {
            Some((key, rest))
                if !key.starts_with('"')
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                (key.to_string(), rest)
            }
            _ => {
                let index = counters[depth];
                counters[depth] += 1;
                (format!("[{index}]"), trimmed)
            }
        };
        stack.push(name);
        let opens = rest.ends_with('{') || rest.ends_with('(') || rest.ends_with('[');
        if opens {
            counters.push(0);
        } else {
            let value = rest.trim_end_matches(',').to_string();
            out.push((stack.join("."), value));
        }
    }
    out
}

/// Differences of two values by field path, at most `max` lines.
fn debug_diff(a: &impl std::fmt::Debug, b: &impl std::fmt::Debug, max: usize) -> Vec<String> {
    let lines_a = debug_lines(a);
    let lines_b = debug_lines(b);
    let map_b: BTreeMap<&str, &str> = lines_b
        .iter()
        .map(|(p, v)| (p.as_str(), v.as_str()))
        .collect();
    let map_a: BTreeMap<&str, &str> = lines_a
        .iter()
        .map(|(p, v)| (p.as_str(), v.as_str()))
        .collect();
    let mut out = Vec::new();
    let mut total = 0;
    for (path, value) in &lines_a {
        let line = match map_b.get(path.as_str()) {
            Some(other) if other == value => continue,
            Some(other) => format!("{}: {} → {}", short_path(path), clip(value), clip(other)),
            None => format!("{}: {} → (fehlt)", short_path(path), clip(value)),
        };
        total += 1;
        if out.len() < max {
            out.push(line);
        }
    }
    for (path, value) in &lines_b {
        if !map_a.contains_key(path.as_str()) {
            total += 1;
            if out.len() < max {
                out.push(format!("{}: (fehlt) → {}", short_path(path), clip(value)));
            }
        }
    }
    if total > out.len() {
        out.push(format!("… {} weitere Felder", total - out.len()));
    }
    out
}

fn short_path(path: &str) -> String {
    path.trim_start_matches("[0].")
        .replace("Some.", "")
        .replace(".[0].", ".")
}

fn clip(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > 70 {
        format!("{}…", chars[..70].iter().collect::<String>())
    } else {
        text.to_string()
    }
}

// --------------------------------------------------------------- schema ----

struct SchemaIndex {
    files: HashMap<String, PathBuf>,
}

impl SchemaIndex {
    fn new(root: &Path) -> Self {
        fn walk(dir: &Path, files: &mut HashMap<String, PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.to_ascii_lowercase().ends_with(".xsd") {
                        files.entry(name.to_string()).or_insert(path);
                    }
                }
            }
        }
        let mut files = HashMap::new();
        walk(root, &mut files);
        SchemaIndex { files }
    }

    /// XSD for a DA XML version (and VersDate for 3.1) and phase.
    fn find(&self, version: &str, version_date: &str, phase: u8) -> Option<&PathBuf> {
        let name = match (version, version_date) {
            ("3.3", _) => format!("GAEB_DA_XML_{phase}_3.3_2021-05.xsd"),
            ("3.2", _) => format!("GAEB_DA_XML_{phase}_3.2_2013-10.xsd"),
            ("3.1", "2009-12") if phase == 84 => "GAEB_DA_XML_84 3.1 2009-12.xsd".to_string(),
            ("3.1", "2009-12") => "GAEB_DA_XML 3.1 2009-12.xsd".to_string(),
            ("3.1", _) if phase == 84 => "GAEB_DA_XML_84.xsd".to_string(),
            ("3.1", _) => "GAEB_DA_XML.xsd".to_string(),
            _ => return None,
        };
        self.files.get(&name)
    }
}

/// Validates all original and written XML files in one go.
fn validate_all(entries: &mut [Entry], schemas: &SchemaIndex) {
    let mut jobs: Vec<(usize, bool, PathBuf, PathBuf)> = Vec::new();
    for (index, entry) in entries.iter_mut().enumerate() {
        if entry.format != Some(Format::Xml) {
            continue;
        }
        if let Some(phase) = entry.sniffed_phase {
            match schemas.find(&entry.sniffed_version, &entry.sniffed_version_date, phase) {
                Some(xsd) => jobs.push((index, false, xsd.clone(), entry.path.clone())),
                None => {
                    entry.schema_original.xsd = format!(
                        "kein XSD für {} {} / {phase}",
                        entry.sniffed_version, entry.sniffed_version_date
                    )
                }
            }
        }
        let written = entry.roundtrip.as_ref().and_then(|rt| rt.written.clone());
        if let (Some(written), Ok(loaded)) = (written, &entry.loaded) {
            match schemas.find("3.3", "", loaded.doc.phase.number()) {
                Some(xsd) => jobs.push((index, true, xsd.clone(), written)),
                None => entry.schema_written.xsd = "kein XSD 3.3 gefunden".into(),
            }
        }
    }
    if jobs.is_empty() {
        return;
    }
    let results = run_validator(&jobs);
    for ((index, written, xsd, _), result) in jobs.iter().zip(results) {
        let schema = Schema {
            ok: result.as_ref().map(|r| r.0),
            xsd: xsd
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string(),
            errors: result.map(|r| r.1).unwrap_or_default(),
        };
        if *written {
            entries[*index].schema_written = schema;
        } else {
            entries[*index].schema_original = schema;
        }
    }
}

const LXML_SCRIPT: &str = r#"
import sys
from lxml import etree
schemas = {}
for line in sys.stdin:
    xsd, path = line.rstrip("\n").split("\t")
    try:
        if xsd not in schemas:
            schemas[xsd] = etree.XMLSchema(etree.parse(xsd))
        schema = schemas[xsd]
        ok = schema.validate(etree.parse(path))
        errors = [f"{e.line}: {e.message}" for e in schema.error_log][:5]
    except Exception as err:
        ok, errors = False, [str(err)]
    print(("OK" if ok else "ERR") + "".join("\t" + e.replace("\t", " ") for e in errors), flush=True)
"#;

type Validation = Option<(bool, Vec<String>)>;

fn run_validator(jobs: &[(usize, bool, PathBuf, PathBuf)]) -> Vec<Validation> {
    let xmllint = Command::new("xmllint").arg("--version").output().is_ok();
    if xmllint {
        return jobs
            .iter()
            .map(|(_, _, xsd, file)| {
                let output = Command::new("xmllint")
                    .arg("--noout")
                    .arg("--schema")
                    .arg(xsd)
                    .arg(file)
                    .output()
                    .ok()?;
                let errors = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .filter(|l| !l.ends_with(" validates"))
                    .take(5)
                    .map(str::to_string)
                    .collect();
                Some((output.status.success(), errors))
            })
            .collect();
    }
    let Ok(mut child) = Command::new("python3")
        .arg("-c")
        .arg(LXML_SCRIPT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return vec![None; jobs.len()];
    };
    let mut input = String::new();
    for (_, _, xsd, file) in jobs {
        let _ = writeln!(input, "{}\t{}", xsd.display(), file.display());
    }
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let Ok(output) = child.wait_with_output() else {
        return vec![None; jobs.len()];
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut results: Vec<Validation> = text
        .lines()
        .map(|line| {
            let mut parts = line.split('\t');
            let ok = parts.next() == Some("OK");
            Some((ok, parts.map(str::to_string).collect()))
        })
        .collect();
    results.resize(jobs.len(), None);
    results
}

// --------------------------------------------------------- cross checks ----

/// OZ key with leading zeros of numeric pieces removed.
fn oz_key(ordinal: &str) -> String {
    ordinal
        .split('.')
        .map(|piece| {
            let t = piece.trim();
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
                let stripped = t.trim_start_matches('0');
                if stripped.is_empty() {
                    "0".to_string()
                } else {
                    stripped.to_string()
                }
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn item_map(doc: &GaebDocument) -> BTreeMap<String, &Item> {
    doc.award
        .boq
        .items()
        .into_iter()
        .map(|(pieces, item)| (oz_key(&pieces.join(".")), item))
        .collect()
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn kind_label(item: &Item) -> String {
    let k = &item.kind;
    let mut parts = Vec::new();
    match k.provisional {
        Some(Provisional::WithTotal) => parts.push("BedarfMitGB".to_string()),
        Some(Provisional::WithoutTotal) => parts.push("BedarfOhneGB".to_string()),
        None => {}
    }
    if let Some(a) = &k.alternative {
        parts.push(format!(
            "Wahl {}/{}",
            a.group.trim().trim_start_matches('0'),
            a.serial.trim().trim_start_matches('0')
        ));
    }
    if k.lump_sum {
        parts.push("Pauschal".into());
    }
    if k.hourly {
        parts.push("Stundenlohn".into());
    }
    if k.markup {
        parts.push("Zulage".into());
    }
    parts.join("+")
}

fn compare_pair(x: &Entry, d: &Entry) -> Vec<String> {
    let (Ok(lx), Ok(ld)) = (&x.loaded, &d.loaded) else {
        return vec![format!(
            "{} ↔ {}: nicht beide lesbar",
            file_name(&x.path),
            file_name(&d.path)
        )];
    };
    let mx = item_map(&lx.doc);
    let md = item_map(&ld.doc);
    let mut fields: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut same = 0;
    for (key, ix) in &mx {
        let Some(id) = md.get(key) else { continue };
        let mut differs = false;
        let mut note = |field: &'static str, detail: String| {
            fields
                .entry(field)
                .or_default()
                .push(format!("{key}: {detail}"));
            differs = true;
        };
        match (ix.qty, id.qty) {
            (Some(a), Some(b)) if a != b => note("Menge", format!("{a} / {b}")),
            (Some(_), None) | (None, Some(_)) => note(
                "Menge nur in einer Datei",
                format!("{:?} / {:?}", ix.qty, id.qty),
            ),
            _ => {}
        }
        if !ix.unit.trim().eq_ignore_ascii_case(id.unit.trim()) {
            note("Einheit", format!("{:?} / {:?}", ix.unit, id.unit));
        }
        if ix.unit_price != id.unit_price {
            note("EP", format!("{:?} / {:?}", ix.unit_price, id.unit_price));
        }
        if ix.total != id.total {
            note("GP", format!("{:?} / {:?}", ix.total, id.total));
        }
        if kind_label(ix) != kind_label(id) {
            note(
                "Positionsart",
                format!("{} / {}", kind_label(ix), kind_label(id)),
            );
        }
        let sx = normalized(&ix.short_text);
        let sd = normalized(&id.short_text);
        if sx != sd {
            if !sd.is_empty() && sx.starts_with(&sd) {
                note(
                    "Kurztext gekürzt",
                    format!("{} Zeichen / {}", sx.chars().count(), sd.chars().count()),
                );
            } else {
                note("Kurztext", format!("{} / {}", clip(&sx), clip(&sd)));
            }
        }
        let lx_text = normalized(&ix.long_text.to_plain());
        let ld_text = normalized(&id.long_text.to_plain());
        if lx_text != ld_text {
            note(
                "Langtext",
                format!(
                    "{} / {} Zeichen",
                    lx_text.chars().count(),
                    ld_text.chars().count()
                ),
            );
        }
        if ix.complements.len() != id.complements.len() {
            note(
                "Textergänzungen",
                format!("{} / {}", ix.complements.len(), id.complements.len()),
            );
        }
        if !differs {
            same += 1;
        }
    }
    let only_x = mx.keys().filter(|k| !md.contains_key(*k)).count();
    let only_d = md.keys().filter(|k| !mx.contains_key(*k)).count();
    let tx = lx.doc.award.boq.totals.as_ref().and_then(|t| t.total);
    let td = ld.doc.award.boq.totals.as_ref().and_then(|t| t.total);
    let mut out = vec![format!(
        "{} ↔ {}: {} / {} Positionen, gleich {same}, nur XML {only_x}, nur D90 {only_d}, Summe {} / {}",
        file_name(&x.path),
        file_name(&d.path),
        mx.len(),
        md.len(),
        tx.map_or("-".into(), |t| t.to_string()),
        td.map_or("-".into(), |t| t.to_string()),
    )];
    for (field, list) in fields {
        out.push(format!(
            "    {field}: {} (z. B. {})",
            list.len(),
            join_some(list.iter().map(String::as_str), 2)
        ));
    }
    out
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

fn cross_checks(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    let mut groups: BTreeMap<(PathBuf, String, Option<u8>), Vec<usize>> = BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let dir = entry.path.parent().unwrap_or(Path::new("")).to_path_buf();
        let stem = entry
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_lowercase();
        groups
            .entry((dir, stem, extension_phase(&entry.path)))
            .or_default()
            .push(index);
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for members in groups.values() {
        for &x in members {
            for &d in members {
                if entries[x].format == Some(Format::Xml)
                    && entries[d].format == Some(Format::Datan)
                {
                    pairs.push((x, d));
                }
            }
        }
    }
    if let Ok(spec) = std::env::var("PROAVA_GAEB_PAIRS") {
        for pair in spec.split(';').filter(|p| !p.trim().is_empty()) {
            let Some((a, b)) = pair.split_once('|') else {
                continue;
            };
            let find = |name: &str| {
                entries
                    .iter()
                    .position(|e| e.path == Path::new(name.trim()) || e.path.ends_with(name.trim()))
            };
            match (find(a), find(b)) {
                (Some(a), Some(b)) => {
                    let (x, d) = if entries[a].format == Some(Format::Xml) {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    pairs.push((x, d));
                }
                _ => out.push(format!("Paar nicht gefunden: {pair}")),
            }
        }
    }
    for (x, d) in pairs {
        out.extend(compare_pair(&entries[x], &entries[d]));
    }
    out
}

/// LV files (81–83) below the bid's folder and up to three parents,
/// nearest level first.
fn lv_candidates(bid: &Path) -> Vec<Vec<PathBuf>> {
    let mut levels = Vec::new();
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut dir = bid.parent();
    for _ in 0..4 {
        let Some(current) = dir else { break };
        let mut found = Vec::new();
        collect(current, &mut found);
        found.retain(|p| matches!(extension_phase(p), Some(81..=83)) && seen.insert(p.clone()));
        found.sort();
        levels.push(found);
        dir = current.parent();
    }
    levels
}

/// The bid with item kinds, missing quantities and surcharge data from the
/// LV: an X84/D84 alone does not say which items are Wahl- or
/// Bedarfspositionen, so its totals can only be checked this way.
fn with_lv_kinds(bid: &GaebDocument, lv: &GaebDocument) -> GaebDocument {
    type Titles = BTreeMap<String, proava_gaeb::Alternative>;
    fn titles(nodes: &[BoqNode], path: &mut Vec<String>, out: &mut Titles) {
        for node in nodes {
            if let BoqNode::Category(category) = node {
                path.push(category.rno_part.clone());
                if let Some(alternative) = &category.alternative {
                    out.insert(oz_key(&path.join(".")), alternative.clone());
                }
                titles(&category.body, path, out);
                path.pop();
            }
        }
    }
    fn walk(
        nodes: &mut [BoqNode],
        path: &mut Vec<String>,
        lv: &BTreeMap<String, &Item>,
        titles: &Titles,
    ) {
        for node in nodes {
            match node {
                BoqNode::Category(category) => {
                    path.push(category.rno_part.clone());
                    if category.alternative.is_none() {
                        category.alternative = titles.get(&oz_key(&path.join("."))).cloned();
                    }
                    walk(&mut category.body, path, lv, titles);
                    path.pop();
                }
                BoqNode::Item(item) => {
                    let Some(source) = lv.get(&oz_key(&ordinal(path, item))) else {
                        continue;
                    };
                    item.kind = source.kind.clone();
                    item.not_applicable |= source.not_applicable;
                    if item.qty.is_none() {
                        item.qty = source.qty;
                    }
                    if item
                        .surcharge
                        .as_ref()
                        .is_none_or(|s| s.markup_type.is_empty())
                    {
                        item.surcharge = source.surcharge.clone();
                    }
                }
                _ => {}
            }
        }
    }
    let mut lv_titles = Titles::new();
    titles(&lv.award.boq.body, &mut Vec::new(), &mut lv_titles);
    let mut doc = bid.clone();
    walk(
        &mut doc.award.boq.body,
        &mut Vec::new(),
        &item_map(lv),
        &lv_titles,
    );
    doc
}

struct BidMatch {
    found: usize,
    bid_items: usize,
    missing: usize,
    phase_rank: u8,
    same_version: bool,
    lv: PathBuf,
    lines: Vec<String>,
}

fn match_bid(bid: &GaebDocument, lv: &GaebDocument, path: &Path) -> BidMatch {
    let bid_items = item_map(bid);
    let lv_items = item_map(lv);
    let unknown: Vec<&str> = bid_items
        .keys()
        .filter(|k| !lv_items.contains_key(*k))
        .map(String::as_str)
        .collect();
    let missing = lv_items
        .keys()
        .filter(|k| !bid_items.contains_key(*k))
        .count();
    let mut qty = Vec::new();
    let mut free_qty = 0;
    let mut units: BTreeMap<(String, String), Vec<&str>> = BTreeMap::new();
    for (key, item) in &bid_items {
        let Some(source) = lv_items.get(key) else {
            continue;
        };
        if item.qty.is_some() && source.qty_to_be_determined {
            free_qty += 1;
        } else if item.qty.is_some() && item.qty != source.qty {
            qty.push(format!(
                "{key} LV {} / Angebot {}",
                source.qty.map_or("-".into(), |q| q.normalize().to_string()),
                item.qty.map_or("-".into(), |q| q.normalize().to_string())
            ));
        }
        if !item.unit.trim().is_empty() && item.unit.trim() != source.unit.trim() {
            units
                .entry((source.unit.trim().to_string(), item.unit.trim().to_string()))
                .or_default()
                .push(key);
        }
    }
    let found = bid_items.len() - unknown.len();
    let mut lines = vec![format!(
        " → {}: {}, {found} von {} OZ im LV ({} Pos.), LV-Positionen ohne Angebot {missing}, Menge abweichend {}, freie Menge (QtyTBD) {free_qty}, Einheit abweichend {}",
        file_name(path),
        if unknown.is_empty() { "ok" } else { "OZ FEHLEN IM LV" },
        bid_items.len(),
        lv_items.len(),
        qty.len(),
        units.values().map(Vec::len).sum::<usize>(),
    )];
    if !unknown.is_empty() {
        lines.push(format!(
            "    nicht im LV: {}",
            join_some(unknown.iter().copied(), 8)
        ));
    }
    if !qty.is_empty() {
        lines.push(format!(
            "    Mengen: {}",
            join_some(qty.iter().map(String::as_str), 5)
        ));
    }
    for ((lv_unit, bid_unit), keys) in &units {
        lines.push(format!(
            "    Einheit LV „{lv_unit}“ / Angebot „{bid_unit}“: {} Pos. (z. B. {})",
            keys.len(),
            join_some(keys.iter().copied(), 3)
        ));
    }
    let merged = with_lv_kinds(bid, lv);
    let computed = xml::computed_total(&merged.award.boq.body, &[]);
    let titles = figures(&merged).category_mismatch;
    if !titles.is_empty() {
        lines.push(format!(
            "    Titelsummen mit LV-Arten abweichend ({}): {}",
            titles.len(),
            join_some(titles.iter().map(String::as_str), 4)
        ));
    }
    let stated = bid.award.boq.totals.as_ref().and_then(|t| t.total);
    lines.push(format!(
        "    Summe mit Positionsarten aus dem LV {} / Datei {} → {}",
        money(Some(computed)),
        money(stated),
        match stated {
            Some(s) if round2(s) == round2(computed) => "gleich".to_string(),
            Some(s) => format!("Δ {:+.2}", computed - s),
            None => "-".to_string(),
        }
    ));
    BidMatch {
        found,
        bid_items: bid_items.len(),
        missing,
        phase_rank: 0,
        same_version: false,
        lv: path.to_path_buf(),
        lines,
    }
}

/// Length of the common start of two file names (to prefer the LV that
/// belongs to the bid when several fit equally well).
fn common_prefix(a: &Path, b: &Path) -> usize {
    let a = file_name(a).to_lowercase();
    let b = file_name(b).to_lowercase();
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// `PROAVA_GAEB_LV`: LV files (absolute paths) that are searched in
/// addition to the folders around each bid, separated by ':'.
fn extra_lvs() -> Vec<PathBuf> {
    std::env::var("PROAVA_GAEB_LV")
        .unwrap_or_default()
        .split(':')
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect()
}

fn bid_checks(entries: &mut [Entry], cache: &mut HashMap<PathBuf, Option<Loaded>>) -> Vec<String> {
    let extra = extra_lvs();
    let mut out = Vec::new();
    for entry in entries.iter_mut() {
        let Ok(bid) = &entry.loaded else { continue };
        if !matches!(bid.doc.phase, Phase::Dp84 | Phase::Dp85) {
            continue;
        }
        let mut levels = lv_candidates(&entry.path);
        levels.push(extra.clone());
        let mut best: Option<BidMatch> = None;
        for level in levels {
            for candidate in level {
                let loaded = cache
                    .entry(candidate.clone())
                    .or_insert_with(|| load_path(&candidate));
                let Some(lv) = loaded else { continue };
                let mut found = match_bid(&bid.doc, &lv.doc, &candidate);
                found.same_version = lv.version == bid.version;
                found.phase_rank = match lv.doc.phase {
                    Phase::Dp83 => 2,
                    Phase::Dp81 => 1,
                    _ => 0,
                };
                // Most OZ found, then fewest LV items without a price, then
                // X83 before X81 before X82, same DA XML version, the most
                // similar file name.
                let rank = |m: &BidMatch| {
                    (
                        m.found,
                        std::cmp::Reverse(m.missing),
                        m.phase_rank,
                        m.same_version,
                        common_prefix(&m.lv, &entry.path),
                    )
                };
                if best.as_ref().is_none_or(|b| rank(&found) > rank(b)) {
                    best = Some(found);
                }
            }
            // Go further up only while some OZ of the bid are still unknown.
            if best.as_ref().is_some_and(|b| b.found == b.bid_items) {
                break;
            }
        }
        match best {
            Some(found) => {
                // R11: the bid (original and as written) against the LV found.
                if let Some(Some(lv)) = cache.get(&found.lv) {
                    let original = rules::check_bid(&bid.doc, &lv.doc);
                    if let Some(rt) = entry.roundtrip.as_mut() {
                        if let Some(again) = &rt.again {
                            rt.rules.extend(rules::check_bid(again, &lv.doc));
                        }
                    }
                    entry.rules.extend(original);
                }
                let mut lines = found.lines;
                lines[0] = format!("{}{}", entry.shown, lines[0]);
                out.extend(lines);
            }
            None => out.push(format!("{}: kein LV (81–83) gefunden", entry.shown)),
        }
    }
    out
}

// ---------------------------------------------------------------- output ----

fn money(value: Option<Decimal>) -> String {
    value.map_or("-".into(), |v| format!("{:.2}", round2(v)))
}

struct Row {
    cells: Vec<String>,
}

fn rows(entries: &[Entry]) -> Vec<Row> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let mut cells = vec![
                format!("{}", index + 1),
                clip_left(&entry.shown, 58),
                entry.format.map_or("?", Format::label).to_string(),
            ];
            match &entry.loaded {
                Ok(loaded) => {
                    let f = &entry.figures;
                    let needs_lv = matches!(loaded.doc.phase, Phase::Dp84 | Phase::Dp85)
                        && loaded
                            .doc
                            .award
                            .boq
                            .items()
                            .iter()
                            .all(|(_, i)| i.qty.is_none());
                    let delta = match f.file_total {
                        _ if needs_lv => "LV nötig".to_string(),
                        Some(total) if round2(total) == round2(f.computed) => "=".to_string(),
                        Some(total) => format!("{:+.2}", f.computed - total),
                        None => "-".to_string(),
                    };
                    let rt = match &entry.roundtrip {
                        Some(rt) if rt.error.is_some() => "FEHLER".to_string(),
                        Some(rt) if rt.ok => "gleich".to_string(),
                        Some(rt) => format!("{} Abw.", rt.diffs.len()),
                        None => "-".to_string(),
                    };
                    cells.extend([
                        loaded.version.clone(),
                        loaded.doc.phase.number().to_string(),
                        f.categories.to_string(),
                        f.items.to_string(),
                        money(f.file_total),
                        money(Some(f.computed)),
                        delta,
                        loaded.warnings.len().to_string(),
                        rt,
                        entry.schema_original.cell().to_string(),
                        entry.schema_written.cell().to_string(),
                        counts_cell(severity_counts(&entry.rules)),
                        counts_cell(severity_counts(entry.new_rules())),
                    ]);
                }
                Err(err) => {
                    let version = if entry.sniffed_version.is_empty() {
                        "-".to_string()
                    } else {
                        entry.sniffed_version.clone()
                    };
                    cells.extend([
                        version,
                        entry.sniffed_phase.map_or("-".into(), |p| p.to_string()),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                        format!("LESEFEHLER: {}", clip(err)),
                        entry.schema_original.cell().to_string(),
                        "-".into(),
                        "-".into(),
                        "-".into(),
                    ]);
                }
            }
            Row { cells }
        })
        .collect()
}

const HEADERS: [&str; 16] = [
    "Nr",
    "Datei",
    "Fmt",
    "Ver",
    "DP",
    "Titel",
    "Pos",
    "Summe Datei",
    "Summe ger.",
    "Δ",
    "Warn",
    "Roundtrip",
    "XSD orig",
    "XSD neu",
    "Regeln orig F/W/H",
    "Regeln neu F/W/H",
];

fn clip_left(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > max {
        format!(
            "…{}",
            chars[chars.len() - max + 1..].iter().collect::<String>()
        )
    } else {
        text.to_string()
    }
}

fn render_table(entries: &[Entry]) -> String {
    let rows = rows(entries);
    let mut widths: Vec<usize> = HEADERS.iter().map(|h| h.chars().count()).collect();
    for row in &rows {
        for (i, cell) in row.cells.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let line = |cells: &[String]| -> String {
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let pad = widths[i] - c.chars().count();
                if (5..=10).contains(&i) || i == 0 {
                    format!("{}{c}", " ".repeat(pad))
                } else {
                    format!("{c}{}", " ".repeat(pad))
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let mut out = String::new();
    let headers: Vec<String> = HEADERS.iter().map(|h| h.to_string()).collect();
    let _ = writeln!(out, "{}", line(&headers));
    let _ = writeln!(
        out,
        "{}",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("-+-")
    );
    for row in &rows {
        let _ = writeln!(out, "{}", line(&row.cells));
    }
    let read = entries.iter().filter(|e| e.loaded.is_ok()).count();
    let rt_ok = entries
        .iter()
        .filter(|e| e.roundtrip.as_ref().is_some_and(|r| r.ok))
        .count();
    let sum_ok = entries
        .iter()
        .filter(|e| {
            e.loaded.is_ok()
                && e.figures
                    .file_total
                    .is_some_and(|t| round2(t) == round2(e.figures.computed))
        })
        .count();
    let xsd_ok = entries
        .iter()
        .filter(|e| e.schema_written.ok == Some(true))
        .count();
    let xsd_checked = entries
        .iter()
        .filter(|e| e.schema_written.ok.is_some())
        .count();
    let _ = write!(
        out,
        "{} Dateien: gelesen {read}, Summe = Datei {sum_ok}, Roundtrip gleich {rt_ok}, geschriebenes XML schemagültig {xsd_ok} von {xsd_checked}",
        entries.len()
    );
    let _ = write!(out, "\n{}", rules_summary(entries));
    out
}

/// One line: Fachregeln of the originals and what proAVA added in writing.
fn rules_summary(entries: &[Entry]) -> String {
    let original = severity_counts(entries.iter().flat_map(|e| e.rules.iter()));
    let files = entries.iter().filter(|e| !e.rules.is_empty()).count();
    let written = severity_counts(entries.iter().flat_map(|e| e.written_rules().iter()));
    let new: Vec<&Finding> = entries.iter().flat_map(Entry::new_rules).collect();
    let new_counts = severity_counts(new.iter().copied());
    let new_files = entries.iter().filter(|e| !e.new_rules().is_empty()).count();
    format!(
        "Fachregeln: Originale {} F / {} W / {} H in {files} Dateien; geschrieben {} F / {} W / {} H, davon neu durch proAVA {} F / {} W / {} H in {new_files} Dateien",
        original[0], original[1], original[2], written[0], written[1], written[2], new_counts[0], new_counts[1], new_counts[2]
    )
}

/// Per rule check: findings and files in the originals, new in written files.
fn rules_table(entries: &[Entry]) -> String {
    #[derive(Default)]
    struct Row {
        severity: Option<Severity>,
        reference: &'static str,
        original: usize,
        original_files: usize,
        new: usize,
        new_files: usize,
        example: String,
    }
    let mut rows: BTreeMap<(u8, &'static str), Row> = BTreeMap::new();
    for entry in entries {
        let mut seen: BTreeSet<&'static str> = BTreeSet::new();
        for finding in &entry.rules {
            let row = rows.entry((finding.rule(), finding.id)).or_default();
            row.severity = Some(finding.severity);
            row.reference = finding.reference;
            row.original += 1;
            if seen.insert(finding.id) {
                row.original_files += 1;
                if row.example.is_empty() {
                    row.example = format!("{}: {}", entry.shown, clip(&finding.text));
                }
            }
        }
        let mut seen_new: BTreeSet<&'static str> = BTreeSet::new();
        for finding in entry.new_rules() {
            let row = rows.entry((finding.rule(), finding.id)).or_default();
            row.severity = Some(finding.severity);
            row.reference = finding.reference;
            row.new += 1;
            if seen_new.insert(finding.id) {
                row.new_files += 1;
                row.example = format!("NEU {}: {}", entry.shown, clip(&finding.text));
            }
        }
    }
    let mut md = String::from(
        "| Regel | Schwere | Fundstelle | Original Befunde (Dateien) | geschrieben neu (Dateien) | Beispiel |\n|---|---|---|---|---|---|\n",
    );
    for ((_, id), row) in rows {
        let _ = writeln!(
            md,
            "| {id} | {} | {} | {} ({}) | {} ({}) | {} |",
            row.severity.map_or("-", Severity::label),
            row.reference,
            row.original,
            row.original_files,
            row.new,
            row.new_files,
            row.example.replace('|', "/")
        );
    }
    md
}

fn render_markdown(
    roots: &[PathBuf],
    entries: &[Entry],
    cross: &[String],
    bids: &[String],
    table: &str,
) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "# GAEB-Prüfstand (gaeb_check)\n");
    let _ = writeln!(md, "Ordner:\n");
    for root in roots {
        let _ = writeln!(md, "- `{}`", root.display());
    }
    let _ = writeln!(md, "\n## Übersicht\n");
    let _ = writeln!(md, "| {} |", HEADERS.join(" | "));
    let _ = writeln!(md, "|{}", "---|".repeat(HEADERS.len()));
    for row in rows(entries) {
        let cells: Vec<String> = row.cells.iter().map(|c| c.replace('|', "/")).collect();
        let _ = writeln!(md, "| {} |", cells.join(" | "));
    }
    for summary in table
        .lines()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let _ = writeln!(md, "\n{summary}");
    }
    let _ = writeln!(
        md,
        "\n## Fachregeln (R1–R13, `proava_gaeb::rules`)\n\nOriginale = fremde Programme (nur Information); „neu“ = Befund der von proAVA geschriebenen Datei, den das Original nicht hat.\n"
    );
    let _ = writeln!(md, "{}", rules_table(entries));
    if !cross.is_empty() {
        let _ = writeln!(
            md,
            "\n## X↔D-Vergleich (gleiche Datei in beiden Formaten)\n"
        );
        let _ = writeln!(md, "```\n{}\n```", cross.join("\n"));
    }
    if !bids.is_empty() {
        let _ = writeln!(md, "\n## Angebot ↔ LV\n");
        for line in bids {
            let _ = writeln!(md, "- {line}");
        }
    }
    let _ = writeln!(md, "\n## Einzelheiten\n");
    for (index, entry) in entries.iter().enumerate() {
        let _ = writeln!(md, "### {} – `{}`\n", index + 1, entry.shown);
        match &entry.loaded {
            Err(err) => {
                let _ = writeln!(md, "- **nicht gelesen:** {err}");
            }
            Ok(loaded) => {
                let doc = &loaded.doc;
                let f = &entry.figures;
                let _ = writeln!(
                    md,
                    "- {} {} DP {}, Zeichensatz {}, Programm „{}“, Datum {}",
                    entry.format.map_or("?", Format::label),
                    loaded.version,
                    doc.phase.number(),
                    loaded.encoding,
                    doc.program,
                    doc.date.as_deref().unwrap_or("-")
                );
                let _ = writeln!(
                    md,
                    "- Projekt „{}“, LV „{}“ / „{}“, Maske {}",
                    doc.project.name,
                    doc.award.boq.name,
                    doc.award.boq.label,
                    doc.award
                        .boq
                        .breakdown
                        .iter()
                        .map(|l| format!("{:?}{}", l.kind, l.length))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                if let Some(bidder) = &doc.award.bidder {
                    let _ = writeln!(md, "- Bieter „{}“", bidder.name1);
                }
                let _ = writeln!(
                    md,
                    "- Titel {}, Positionen {}, Hinweistexte {}, Ausführungsbeschreibungen {}",
                    f.categories, f.items, f.remarks, f.perf_descr
                );
                let _ = writeln!(
                    md,
                    "- Summe Datei {}, gerechnet (xml::computed_total) {}, Σ GP wie geschrieben {}",
                    money(f.file_total),
                    money(Some(f.computed)),
                    money(Some(f.it_sum))
                );
                if !f.category_mismatch.is_empty() {
                    let _ = writeln!(
                        md,
                        "- Titelsummen abweichend ({}): {}",
                        f.category_mismatch.len(),
                        join_some(f.category_mismatch.iter().map(String::as_str), 4)
                    );
                }
                if !f.it_mismatch.is_empty() {
                    let _ = writeln!(
                        md,
                        "- GP ≠ Menge × EP ({}): {}",
                        f.it_mismatch.len(),
                        join_some(f.it_mismatch.iter().map(String::as_str), 4)
                    );
                }
                if !loaded.warnings.is_empty() {
                    let _ = writeln!(md, "- Hinweise beim Lesen ({}):", loaded.warnings.len());
                    for warning in loaded.warnings.iter().take(10) {
                        let _ = writeln!(md, "  - {warning}");
                    }
                }
            }
        }
        for note in &entry.notes {
            let _ = writeln!(md, "- Hinweis: {note}");
        }
        if let Some(rt) = &entry.roundtrip {
            if let Some(err) = &rt.error {
                let _ = writeln!(md, "- **Roundtrip FEHLER:** {err}");
            } else if rt.ok {
                let _ = writeln!(md, "- Roundtrip: Modell gleich ({})", rt.note);
            } else {
                let _ = writeln!(md, "- **Roundtrip: Unterschiede** ({})", rt.note);
                let _ = writeln!(md, "  ```");
                for diff in &rt.diffs {
                    let _ = writeln!(md, "  {diff}");
                }
                let _ = writeln!(md, "  ```");
            }
        }
        if let Some(rt) = entry.roundtrip.as_ref().filter(|rt| !rt.dropped.is_empty()) {
            let _ = writeln!(
                md,
                "- Original → geschrieben (gewollt verkürzt/normalisiert, {} Zeilen):",
                rt.dropped.len()
            );
            let _ = writeln!(md, "  ```");
            for diff in rt.dropped.iter().take(25) {
                let _ = writeln!(md, "  {diff}");
            }
            let _ = writeln!(md, "  ```");
        }
        if !entry.rules.is_empty() {
            let _ = writeln!(md, "- Fachregeln Original ({}):", entry.rules.len());
            for finding in entry.rules.iter().take(15) {
                let _ = writeln!(md, "  - {finding}");
            }
            if entry.rules.len() > 15 {
                let _ = writeln!(md, "  - … (+{})", entry.rules.len() - 15);
            }
        }
        let new = entry.new_rules();
        if !new.is_empty() {
            let _ = writeln!(
                md,
                "- **Fachregeln geschrieben, neu durch proAVA ({}):**",
                new.len()
            );
            for finding in new.iter().take(15) {
                let _ = writeln!(md, "  - {finding}");
            }
        }
        for (label, schema) in [
            ("XSD Original", &entry.schema_original),
            ("XSD geschrieben", &entry.schema_written),
        ] {
            if schema.ok.is_some() || !schema.xsd.is_empty() {
                let _ = writeln!(md, "- {label}: {} ({})", schema.cell(), schema.xsd);
                for error in &schema.errors {
                    let _ = writeln!(md, "  - {error}");
                }
            }
        }
        let _ = writeln!(md);
    }
    md
}
