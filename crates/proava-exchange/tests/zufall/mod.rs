//! Zufällige, aber gültige Projekte für die Eigenschaftstests: mehrere LVs
//! mit Titeln, Positionen aller Arten, Hinweisen, Ausführungsbeschreibungen
//! und Bezügen, Abstandshaltern, RichText mit allen Blockarten und
//! Sonderzeichen, Bilder und PDF-Ausschnitte, Angebote, Bieterverzeichnis,
//! Stammdaten, Schriftverkehr im Projekt und Kostenermittlungen.
//!
//! Feste Zufallssaat (siehe [`config`]); längere Läufe über
//! `PROAVA_PROPTEST_CASES` und `PROAVA_PROPTEST_SEED` (docs/BUILD.md).

#![allow(dead_code)]

use proava_domain::{
    Address, Align, Bid, BidComplement, BidPrice, Cell, ClipRect, ComplementKind,
    CorrespondenceEntry, CorrespondenceKind, CostEstimate, CostSection, CostShare, CostStage,
    Decimal, Discount, EditStatus, InviteStatus, InvitedBidder, InviteeId, Lv, LvGroup, LvRow,
    Note, OzMask, PdfClip, PerformanceDescription, PointKind, PointRow, Position, PositionKind,
    Project, ProjectName, RichText, Run, Spacer, Table, TextBlock, TextComplement,
};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

/// Test runner settings: fixed seed, `PROAVA_PROPTEST_CASES`/`_SEED` for
/// longer runs.
pub fn config(default_cases: u32) -> Config {
    let cases = std::env::var("PROAVA_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default_cases);
    let seed = std::env::var("PROAVA_PROPTEST_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x0A7A_2026);
    Config {
        cases,
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        max_shrink_iters: 2048,
        ..Config::default()
    }
}

/// Pieces texts are made of: words, spaces, line breaks, XML and GAEB
/// special characters, umlauts, typographic characters, emoji, combining
/// characters and invisible ones (soft hyphen, zero width space).
pub const PIECES: &[&str] = &[
    "Wort",
    "DDC",
    "Schaltschrank",
    "a",
    "1,5",
    "0,00",
    "100 %",
    " ",
    " ",
    "  ",
    "\n",
    "\t",
    "&",
    "<",
    ">",
    "\"",
    "'",
    "]]>",
    "&amp;",
    "<b>",
    "<!--",
    "ä",
    "Ö",
    "ü",
    "ß",
    "€",
    "§",
    "°",
    "²",
    "µ",
    "½",
    "±",
    "Ø",
    "–",
    "—",
    "„",
    "“",
    "‚",
    "‘",
    "…",
    "•",
    "\u{a0}",
    "\u{ad}",
    "\u{200b}",
    "😀",
    "e\u{301}",
    "\\",
    "{",
    "}",
    "#",
    "$",
    "@",
    "*",
    "_",
    "/",
    "|",
    "~",
    "^",
];

/// Control characters a pasted text can bring along (Word, PDF): XML 1.0
/// cannot hold them directly.
pub const CONTROL: &[&str] = &["\r\n", "\r", "\u{b}", "\u{c}", "\u{1}", "\u{1b}", "\u{7f}"];

pub const UNITS: &[&str] = &[
    "St", "m", "m²", "m³", "psch", "h", "kg", "lfdm", "Stck", "Wo",
];
pub const MASKS: &[&str] = &[
    "11.22.PPPP",
    "11.22.PPPPI",
    "11.222.33.PPPP",
    "1.PPP",
    "LL.22.PPPP",
    "PPPP",
];
pub const STEPS: &[u32] = &[1, 10, 100];

/// Which characters texts may contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    /// Everything, including control characters.
    All,
    /// Without control characters.
    Printable,
}

pub fn text(alphabet: Alphabet, max: usize) -> impl Strategy<Value = String> {
    let mut pieces: Vec<&'static str> = PIECES.to_vec();
    if alphabet == Alphabet::All {
        pieces.extend_from_slice(CONTROL);
    }
    prop::collection::vec(
        prop_oneof![
            4 => prop::sample::select(pieces).prop_map(str::to_string),
            1 => "[a-zA-Z0-9äöüÄÖÜß]{1,8}",
        ],
        0..max,
    )
    .prop_map(|pieces| pieces.concat())
}

/// A single line (titles, short texts, names).
pub fn line(alphabet: Alphabet, max: usize) -> impl Strategy<Value = String> {
    text(alphabet, max)
}

fn align() -> impl Strategy<Value = Align> {
    prop::sample::select(Align::ALL.to_vec())
}

fn run(alphabet: Alphabet) -> impl Strategy<Value = Run> {
    (
        text(alphabet, 6),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        prop::option::of(prop::sample::select(vec![
            "Arial",
            "Times New Roman",
            "Courier New",
            "Bahnschrift",
        ])),
        prop::option::of(prop::sample::select(vec![
            8.0f32, 8.5, 10.0, 11.0, 14.0, 33.3,
        ])),
    )
        .prop_map(|(text, bold, italic, underline, font, size_pt)| Run {
            text,
            bold,
            italic,
            underline,
            font: font.map(str::to_string),
            size_pt,
        })
}

fn runs(alphabet: Alphabet) -> impl Strategy<Value = Vec<Run>> {
    prop::collection::vec(run(alphabet), 0..4)
}

fn table(alphabet: Alphabet) -> impl Strategy<Value = Table> {
    (1usize..4, 1usize..4).prop_flat_map(move |(rows, columns)| {
        (
            prop::collection::vec(
                prop::collection::vec(
                    (runs(alphabet), align()).prop_map(|(runs, align)| Cell { runs, align }),
                    columns,
                ),
                rows,
            ),
            0..=rows,
            prop::collection::vec(
                prop::option::of(prop::sample::select(vec![10.0f32, 42.5, 62.5])),
                0..=columns,
            ),
        )
            .prop_map(|(rows, header_rows, column_widths_mm)| Table {
                rows,
                header_rows,
                column_widths_mm,
            })
    })
}

/// Image names the generator uses; [`add_images`] puts them in the project.
pub const IMAGES: &[&str] = &["schrank.png", "detail_2.jpg", "Bild_ä_1.png"];

fn block(alphabet: Alphabet) -> impl Strategy<Value = TextBlock> {
    prop_oneof![
        5 => (runs(alphabet), align()).prop_map(|(runs, align)| TextBlock::Paragraph { runs, align }),
        2 => (any::<bool>(), 0u8..=3, runs(alphabet), align())
            .prop_map(|(ordered, level, runs, align)| TextBlock::ListItem { ordered, level, runs, align }),
        1 => table(alphabet).prop_map(TextBlock::Table),
        1 => (
            prop::sample::select(IMAGES.to_vec()),
            prop::option::of(prop::sample::select(vec![20.0f32, 62.5, 100.0, 33.3]))
        )
            .prop_map(|(file, width_mm)| TextBlock::Image { file: file.to_string(), width_mm }),
    ]
}

pub fn rich_text(alphabet: Alphabet) -> impl Strategy<Value = RichText> {
    prop::collection::vec(block(alphabet), 0..5).prop_map(RichText::new)
}

fn address(alphabet: Alphabet) -> impl Strategy<Value = Address> {
    (
        (
            line(alphabet, 4),
            line(alphabet, 3),
            line(alphabet, 3),
            "[0-9]{0,5}",
            line(alphabet, 3),
            line(alphabet, 2),
        ),
        (
            line(alphabet, 2),
            line(alphabet, 2),
            line(alphabet, 2),
            line(alphabet, 2),
            "[0-9]{0,13}",
            "(DE[0-9]{9})?",
        ),
    )
        .prop_map(
            |(
                (name1, name2, street, postal_code, city, email),
                (name3, name4, country, contact, iln, vat_id),
            )| Address {
                name1,
                name2,
                name3,
                name4,
                street,
                postal_code,
                city,
                country,
                iln,
                contact,
                phone: "+49 203 0".into(),
                fax: "+49 203 1".into(),
                email,
                vat_id,
            },
        )
}

/// One row of an LV before numbering.
#[derive(Debug, Clone)]
pub enum RowSpec {
    Title {
        deeper: bool,
        text: String,
    },
    Position(PositionSpec),
    Note {
        text: RichText,
        heading: String,
        preliminary: bool,
    },
    Performance {
        label: String,
        text: RichText,
    },
    /// Abstandshalter (empty line for editing only).
    Spacer,
}

#[derive(Debug, Clone)]
pub struct PositionSpec {
    pub short_text: String,
    pub long_text: RichText,
    pub quantity: (i64, u32),
    pub unit: &'static str,
    pub price: Option<(i64, u32)>,
    pub kind: u8,
    pub flags: u8,
    pub status: u8,
    pub performance: Option<prop::sample::Index>,
    pub repeats: Option<prop::sample::Index>,
    pub complements: Vec<ComplementSpec>,
}

/// Textergänzung: Auftraggeber (sonst Bieter), Ansprache, Wert, Anker
/// (Block, Versatz, eigene Zeile).
pub type ComplementSpec = (bool, String, String, Option<(usize, usize, bool)>);

fn position_spec(alphabet: Alphabet) -> impl Strategy<Value = PositionSpec> {
    (
        (line(alphabet, 6), rich_text(alphabet)),
        (
            (0i64..1_000_000, 0u32..4),
            prop::sample::select(UNITS.to_vec()),
        ),
        prop::option::of((-500_000i64..5_000_000, 0u32..4)),
        (0u8..8, any::<u8>(), 0u8..3),
        prop::option::of(any::<prop::sample::Index>()),
        prop::option::of(any::<prop::sample::Index>()),
        prop::collection::vec(
            (
                any::<bool>(),
                line(alphabet, 3),
                line(alphabet, 3),
                prop::option::of((0usize..6, 0usize..30, any::<bool>())),
            ),
            0..3,
        ),
    )
        .prop_map(
            |(
                (short_text, long_text),
                (quantity, unit),
                price,
                (kind, flags, status),
                performance,
                repeats,
                complements,
            )| {
                PositionSpec {
                    short_text,
                    long_text,
                    quantity,
                    unit,
                    price,
                    kind,
                    flags,
                    status,
                    performance,
                    repeats,
                    complements,
                }
            },
        )
}

fn row_spec(alphabet: Alphabet) -> impl Strategy<Value = RowSpec> {
    prop_oneof![
        3 => (any::<bool>(), line(alphabet, 5)).prop_map(|(deeper, text)| RowSpec::Title { deeper, text }),
        6 => position_spec(alphabet).prop_map(RowSpec::Position),
        1 => (rich_text(alphabet), line(alphabet, 3), any::<bool>())
            .prop_map(|(text, heading, preliminary)| RowSpec::Note { text, heading, preliminary }),
        1 => (line(alphabet, 3), rich_text(alphabet)).prop_map(|(label, text)| RowSpec::Performance { label, text }),
        1 => Just(RowSpec::Spacer),
    ]
}

#[derive(Debug, Clone)]
pub struct LvSpec {
    pub mask: usize,
    pub step: usize,
    pub title: String,
    pub rows: Vec<RowSpec>,
    /// Versandstand: X83 time, fingerprint of what the bidders got, file.
    pub sent: Option<(String, String, String)>,
}

fn lv_spec(alphabet: Alphabet, max_rows: usize) -> impl Strategy<Value = LvSpec> {
    (
        0..MASKS.len(),
        0..STEPS.len(),
        line(alphabet, 4),
        prop::collection::vec(row_spec(alphabet), 0..max_rows),
        prop::option::weighted(
            0.3,
            (line(alphabet, 2), "[0-9a-f]{0,16}", line(alphabet, 3)),
        ),
    )
        .prop_map(|(mask, step, title, rows, sent)| LvSpec {
            mask,
            step,
            title,
            rows,
            sent,
        })
}

/// One bid price: which position, unit price (mantissa, scale), not offered.
pub type PriceSpec = (prop::sample::Index, Option<(i64, u32)>, bool);

#[derive(Debug, Clone)]
pub struct BidSpec {
    pub bidder: Address,
    pub prices: Vec<PriceSpec>,
    pub discount: u8,
    pub excluded: bool,
    pub complement: Option<(prop::sample::Index, String)>,
    /// Angebotsmengen that differ from the LV (Massenmehrung/-minderung).
    pub quantities: Vec<(prop::sample::Index, (i64, u32))>,
}

fn bid_spec(alphabet: Alphabet) -> impl Strategy<Value = BidSpec> {
    (
        address(alphabet),
        prop::collection::vec(
            (
                any::<prop::sample::Index>(),
                prop::option::of((0i64..10_000_000, 0u32..4)),
                any::<bool>(),
            ),
            0..8,
        ),
        0u8..3,
        any::<bool>(),
        prop::option::of((any::<prop::sample::Index>(), line(alphabet, 3))),
        prop::collection::vec(
            (any::<prop::sample::Index>(), (0i64..1_000_000, 0u32..4)),
            0..3,
        ),
    )
        .prop_map(
            |(bidder, prices, discount, excluded, complement, quantities)| BidSpec {
                bidder,
                prices,
                discount,
                excluded,
                complement,
                quantities,
            },
        )
}

#[derive(Debug, Clone)]
pub struct ProjectSpec {
    pub lvs: Vec<LvSpec>,
    pub info: (String, String, String, String, Address, Address),
    pub bids: Vec<BidSpec>,
    pub invitees: Vec<(Address, String, u8)>,
    pub costs: Vec<CostEstimate>,
    pub letters: Vec<CorrespondenceEntry>,
    pub clips: Vec<ClipSpec>,
}

pub fn project_spec(
    alphabet: Alphabet,
    max_lvs: usize,
    max_rows: usize,
) -> impl Strategy<Value = ProjectSpec> {
    (
        prop::collection::vec(lv_spec(alphabet, max_rows), 1..=max_lvs),
        (
            line(alphabet, 4),
            line(alphabet, 3),
            line(alphabet, 2),
            line(alphabet, 3),
            address(alphabet),
            address(alphabet),
        ),
        prop::collection::vec(bid_spec(alphabet), 0..3),
        prop::collection::vec((address(alphabet), line(alphabet, 3), 0u8..3), 0..3),
        prop::collection::vec(cost_estimate(alphabet), 0..3),
        prop::collection::vec(letter(alphabet), 0..3),
        prop::collection::vec(clip_spec(alphabet), 0..3),
    )
        .prop_map(
            |(lvs, info, bids, invitees, costs, letters, clips)| ProjectSpec {
                lvs,
                info,
                bids,
                invitees,
                costs,
                letters,
                clips,
            },
        )
}

/// A decimal with up to `scale` decimals, also negative.
fn amount(max: i64, scale: u32) -> impl Strategy<Value = Decimal> {
    (-max..max, 0..=scale).prop_map(|(mantissa, scale)| Decimal::new(mantissa, scale))
}

/// A Kostenschätzung or -berechnung with every field filled at random (as a
/// file could hold it – also odd codes, percentages ≠ 100 and spans upside down).
pub fn cost_estimate(alphabet: Alphabet) -> impl Strategy<Value = CostEstimate> {
    let kind = (line(alphabet, 2), line(alphabet, 3), amount(100_000_000, 3));
    let share = move || (line(alphabet, 2), amount(20_000, 2));
    let row = (
        line(alphabet, 2),
        line(alphabet, 2),
        line(alphabet, 2),
        line(alphabet, 3),
        prop::collection::vec(any::<u32>(), 8),
    );
    (
        (
            any::<bool>(),
            line(alphabet, 4),
            line(alphabet, 2),
            line(alphabet, 2),
            text(alphabet, 4),
        ),
        prop::collection::vec(kind, 0..7),
        prop::collection::vec(
            (
                line(alphabet, 3),
                prop::option::of(prop::collection::vec(share(), 0..7)),
                prop::collection::vec(row, 0..5),
            ),
            0..4,
        ),
        (
            amount(100_000_000, 3),
            prop::option::of(amount(100_000_000, 3)),
            prop::option::of(amount(100_000_000, 3)),
            any::<bool>(),
        ),
        prop::collection::vec(share(), 0..7),
    )
        .prop_map(
            |(
                (calculation, title, date, author, remark),
                kinds,
                sections,
                (rate, low, high, per_kind),
                shares,
            )| {
                let stage = if calculation {
                    CostStage::Calculation
                } else {
                    CostStage::Estimate
                };
                let mut estimate = CostEstimate::template(stage, "", "");
                estimate.title = title;
                estimate.date = date;
                estimate.author = author;
                estimate.remark = remark;
                estimate.kinds = kinds
                    .into_iter()
                    .map(|(key, label, unit_price)| PointKind {
                        key,
                        label,
                        unit_price,
                    })
                    .collect();
                let count = estimate.kinds.len();
                let share_of = |(code, percent): (String, Decimal)| CostShare { code, percent };
                estimate.sections = sections
                    .into_iter()
                    .map(|(name, own, rows)| CostSection {
                        name,
                        shares: own.map(|shares| shares.into_iter().map(share_of).collect()),
                        rows: rows
                            .into_iter()
                            .map(|(location, trade, plant, remark, mut counts)| {
                                // As many counts as kinds (the editor keeps them in step).
                                counts.truncate(count);
                                PointRow {
                                    location,
                                    trade,
                                    plant,
                                    remark,
                                    counts,
                                }
                            })
                            .collect(),
                    })
                    .collect();
                estimate.rate = rate;
                estimate.rate_low = low;
                estimate.rate_high = high;
                estimate.rate_per_kind = per_kind;
                estimate.shares = shares.into_iter().map(share_of).collect();
                estimate
            },
        )
}

/// A filed letter (Schriftverkehr im Projekt), also without its PDF.
pub fn letter(alphabet: Alphabet) -> impl Strategy<Value = CorrespondenceEntry> {
    (
        (
            0..CorrespondenceKind::ALL.len(),
            line(alphabet, 2),
            line(alphabet, 3),
            prop::option::of(0u64..5),
        ),
        (
            line(alphabet, 3),
            line(alphabet, 2),
            line(alphabet, 4),
            line(alphabet, 3),
            line(alphabet, 3),
        ),
        prop::collection::vec(any::<u8>(), 0..64),
        0u64..100_000,
    )
        .prop_map(
            |(
                (kind, lv_number, lv_title, recipient),
                (recipient_name, date, subject, file, remark),
                pdf,
                size,
            )| {
                let mut entry = CorrespondenceEntry::new(
                    CorrespondenceKind::ALL[kind],
                    date,
                    subject,
                    pdf.clone(),
                );
                entry.lv_number = lv_number;
                entry.lv_title = lv_title;
                entry.recipient = recipient.map(InviteeId);
                entry.recipient_name = recipient_name;
                entry.file = file;
                entry.remark = remark;
                if pdf.is_empty() {
                    // The size stays known when the project file lacked the PDF.
                    entry.size = size;
                }
                entry
            },
        )
}

/// A PDF clip: which of two PDFs, page, page size, rectangle, preview choices.
#[derive(Debug, Clone)]
pub struct ClipSpec {
    pub pdf: u8,
    pub clip: PdfClip,
}

fn clip_spec(alphabet: Alphabet) -> impl Strategy<Value = ClipSpec> {
    let size = || prop_oneof![Just(595.27f32), Just(841.89), Just(2383.94), 1.0f32..5000.0];
    (
        0u8..2,
        (0u32..4, size(), size()),
        (0.0f32..1.0, 0.0f32..1.0, 0.0f32..1.0, 0.0f32..1.0),
        line(alphabet, 3),
        (any::<bool>(), any::<bool>()),
        prop::collection::vec(any::<u8>(), 1..40),
    )
        .prop_map(
            |(pdf, (page, width, height), (left, top, w, h), source, (frozen, hidden), png)| {
                let rect = ClipRect::new(left * width, top * height, w * width, h * height);
                let mut clip = PdfClip::new(page, (width, height), rect, source);
                clip.preview_hidden = hidden;
                // Frozen only with its picture (the file keeps no frozen clip without one).
                clip.preview_frozen = frozen;
                clip.frozen_png = frozen.then(|| std::sync::Arc::new(png));
                ClipSpec { pdf, clip }
            },
        )
}

fn decimal((mantissa, scale): (i64, u32)) -> Decimal {
    Decimal::new(mantissa, scale)
}

/// Whether the LV is ready for GAEB: positions only on the deepest title
/// level and with a short text. proAVA allows both; the export then
/// inserts empty levels or the import takes the first long text line – a
/// documented change with a warning on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Levels {
    Free,
    Gaeb,
}

/// The LV a spec describes, numbered like the editor does.
pub fn build_lv(spec: &LvSpec, number: usize, levels_rule: Levels) -> Lv {
    let mask = OzMask::parse(MASKS[spec.mask]).unwrap();
    let levels = mask.level_count();
    let mut lv = Lv::new(spec.title.clone(), mask);
    lv.set_position_step(STEPS[spec.step]);
    lv.set_number(format!("{number}"));
    let mut level: Option<usize> = None;
    let mut titles_on_level = vec![0u32; levels.max(1)];
    let mut performance_count = 0;
    let mut pending: Vec<(proava_domain::RowId, &PositionSpec)> = Vec::new();
    for row in &spec.rows {
        match row {
            RowSpec::Title { deeper, text } => {
                if levels == 0 {
                    continue;
                }
                let next = match level {
                    Some(current) if *deeper && current + 1 < levels => current + 1,
                    Some(current) if *deeper => current,
                    Some(_) | None => 0,
                };
                // Never more titles on a level than its digits allow.
                if titles_on_level[next] >= lv.oz_mask().level_max(next).min(9) {
                    continue;
                }
                titles_on_level[next] += 1;
                for deeper in titles_on_level.iter_mut().skip(next + 1) {
                    *deeper = 0;
                }
                level = Some(next);
                lv.push_group(LvGroup::new(next, "", text.clone()));
            }
            RowSpec::Position(spec) => {
                if levels_rule == Levels::Gaeb && levels > 0 {
                    // Titles down to the deepest level first.
                    let mut blocked = false;
                    while level != Some(levels - 1) {
                        let next = level.map_or(0, |current| current + 1);
                        if titles_on_level[next] >= lv.oz_mask().level_max(next).min(9) {
                            blocked = true;
                            break;
                        }
                        titles_on_level[next] += 1;
                        for deeper in titles_on_level.iter_mut().skip(next + 1) {
                            *deeper = 0;
                        }
                        level = Some(next);
                        lv.push_group(LvGroup::new(next, "", format!("Stufe {next}")));
                    }
                    if blocked {
                        continue;
                    }
                }
                let short_text = if levels_rule == Levels::Gaeb && spec.short_text.trim().is_empty()
                {
                    format!("Kurztext{}", spec.short_text)
                } else {
                    spec.short_text.clone()
                };
                let mut position = Position::new(
                    "",
                    short_text,
                    decimal(spec.quantity),
                    spec.unit,
                    spec.price.map(decimal),
                );
                position.set_long_text(spec.long_text.clone());
                position.set_kind(match spec.kind {
                    1 => PositionKind::Provisional { with_total: true },
                    2 => PositionKind::Provisional { with_total: false },
                    3 => PositionKind::Supplement,
                    4 => PositionKind::Base { group: 1 },
                    5 => PositionKind::Alternative {
                        group: 1,
                        serial: 1,
                    },
                    _ => PositionKind::Normal,
                });
                position.set_lump_sum(spec.flags & 1 != 0 && spec.kind == 0);
                position.set_hourly(spec.flags & 2 != 0 && spec.kind == 0 && spec.flags & 1 == 0);
                position.set_quantity_open(spec.flags & 4 != 0);
                position.set_not_applicable(spec.flags & 8 != 0);
                position.set_key_item(spec.flags & 16 != 0);
                position.set_price_breakdown(spec.flags & 32 != 0);
                position.set_complements(
                    spec.complements
                        .iter()
                        .enumerate()
                        .map(|(index, (owner, caption, body, anchor))| TextComplement {
                            marker: format!("{:02}", index + 1),
                            kind: if *owner {
                                ComplementKind::Owner
                            } else {
                                ComplementKind::Bidder
                            },
                            caption: caption.clone(),
                            body: if *owner { body.clone() } else { String::new() },
                            tail: String::new(),
                            // Bieterangabe as a field in the long text (own line: offset 0).
                            anchor: anchor.map(|(block, offset, own_line)| {
                                proava_domain::TextAnchor {
                                    block,
                                    offset: if own_line { 0 } else { offset },
                                    own_line,
                                }
                            }),
                        })
                        .collect(),
                );
                let id = lv.push_position(position);
                pending.push((id, spec));
            }
            RowSpec::Note {
                text,
                heading,
                preliminary,
            } => {
                let mut note = Note::rich(text.clone());
                note.set_heading(heading.clone());
                note.set_preliminary(*preliminary);
                lv.push_note(note);
            }
            RowSpec::Performance { label, text } => {
                performance_count += 1;
                lv.push_row(LvRow::Performance(PerformanceDescription::new(
                    format!("{performance_count}"),
                    label.clone(),
                    text.clone(),
                )));
            }
            RowSpec::Spacer => {
                lv.push_row(LvRow::Spacer(Spacer::new()));
            }
        }
    }
    lv.renumber();
    // References: performance descriptions and repetitions.
    let performances: Vec<_> = lv
        .performances()
        .map(|performance| performance.id())
        .collect();
    let positions: Vec<_> = lv.positions().map(|position| position.id()).collect();
    for (id, spec) in &pending {
        if let Some(index) = &spec.performance {
            if !performances.is_empty() {
                let _ =
                    lv.link_performance(*id, Some(performances[index.index(performances.len())]));
            }
        }
        if let Some(index) = &spec.repeats {
            let base = positions[index.index(positions.len())];
            if base != *id {
                let _ = lv.link_repetition(*id, Some(base));
            }
        }
    }
    // Status last: a finished position keeps its number from now on.
    let statuses: Vec<_> = pending
        .iter()
        .map(|(id, spec)| {
            (
                *id,
                [EditStatus::Open, EditStatus::InProgress, EditStatus::Done][spec.status as usize],
            )
        })
        .collect();
    for (id, status) in statuses {
        lv.set_status(&[id], status);
    }
    if let Some((exported, fingerprint, file)) = &spec.sent {
        lv.set_request_exported(Some(exported.clone()));
        lv.set_sent_fingerprint(Some(fingerprint.clone()));
        lv.set_sent_file(Some(file.clone()));
    }
    lv
}

/// Small image files (bytes only matter for the project file).
pub fn add_images(project: &mut Project) {
    for (index, name) in IMAGES.iter().enumerate() {
        let bytes: Vec<u8> = (0..=255u8)
            .cycle()
            .skip(index * 7)
            .take(40 + index * 50)
            .collect();
        let got = project.add_image(name, bytes);
        assert_eq!(&got, name, "Bildname");
    }
}

pub fn build_project(spec: &ProjectSpec, levels: Levels) -> Project {
    let mut lvs = spec.lvs.iter().enumerate().map(|(index, lv)| {
        let mut built = build_lv(lv, index + 1, levels);
        // Some further LVs are Kosten-LVs (Kostenermittlung as Kurztext-LV):
        // the project file must keep that. Not in GAEB projects – GAEB does
        // not carry it – and never the first (active) LV.
        if levels == Levels::Free && index > 0 && STEPS[lv.step] == 100 {
            let stage = if lv.mask % 2 == 0 {
                proava_domain::CostStage::Estimate
            } else {
                proava_domain::CostStage::Calculation
            };
            built.set_purpose(proava_domain::LvPurpose::Cost(stage));
        }
        built
    });
    let first = lvs.next().expect("mindestens ein LV");
    let mut project = Project::new(
        ProjectName::parse("Zufall").unwrap(),
        "2026-11-17T21:00:00",
        first,
    );
    for lv in lvs {
        project.add_lv(lv);
    }
    add_images(&mut project);
    let (label, short_name, award_number, deadline, owner, planner) = spec.info.clone();
    let info = project.info_mut();
    info.label = label;
    info.short_name = short_name;
    info.award_number = award_number;
    info.submission_deadline = deadline;
    info.owner = owner;
    info.planner = planner;

    let positions: Vec<_> = project
        .lv()
        .positions()
        .map(|position| position.id())
        .collect();
    let mut bid_ids = Vec::new();
    for spec in &spec.bids {
        let mut bid = Bid::new(spec.bidder.clone(), "zufall.X84");
        if !positions.is_empty() {
            for (index, price, not_offered) in &spec.prices {
                bid.set_price(
                    positions[index.index(positions.len())],
                    BidPrice {
                        unit_price: price.map(decimal),
                        stated_total: None,
                        not_offered: *not_offered,
                    },
                );
            }
            if let Some((index, text)) = &spec.complement {
                bid.set_complements(
                    positions[index.index(positions.len())],
                    vec![BidComplement {
                        marker: "01".into(),
                        text: text.clone(),
                    }],
                );
            }
        }
        bid.set_discount(match spec.discount {
            1 => Discount::Percent(Decimal::new(35, 1)),
            2 => Discount::Amount(proava_domain::Money::from_cents(-12345)),
            _ => Discount::None,
        });
        bid.set_excluded(spec.excluded);
        let id = project.add_bid(bid);
        // Angebotsmengen exist for priced positions (taken when the bid came in).
        if let Some(bid) = project.bid_mut(id) {
            // Sorted: the order of a HashMap differs from run to run.
            let mut priced: Vec<_> = bid.prices().keys().copied().collect();
            priced.sort_by_key(|id| id.0);
            for (index, quantity) in &spec.quantities {
                if !priced.is_empty() {
                    bid.set_offered_quantity(
                        priced[index.index(priced.len())],
                        Some(decimal(*quantity)),
                    );
                }
            }
        }
        bid_ids.push(id);
    }
    for (index, (address, remark, status)) in spec.invitees.iter().enumerate() {
        let mut invitee = InvitedBidder::new(address.clone());
        invitee.remark = remark.clone();
        invitee.status = [
            InviteStatus::Requested,
            InviteStatus::NoBid,
            InviteStatus::Excluded,
        ][*status as usize];
        let id = project.add_invitee(invitee);
        if let Some(bid) = bid_ids.get(index) {
            let _ = project.link_bid(*bid, Some(id));
        }
    }
    for estimate in &spec.costs {
        project.add_cost_estimate(estimate.clone());
    }
    for letter in &spec.letters {
        project.add_correspondence(letter.clone());
    }
    // Two different PDFs; clips of the same PDF are stored once in the file.
    for clip in &spec.clips {
        let pdf: Vec<u8> = format!("%PDF-1.7 Zufall {}", clip.pdf).into_bytes();
        project.add_pdf_clip("plan.pdf", pdf, clip.clip.clone());
    }
    project
}

pub fn project(
    alphabet: Alphabet,
    levels: Levels,
    max_lvs: usize,
    max_rows: usize,
) -> impl Strategy<Value = Project> {
    project_spec(alphabet, max_lvs, max_rows).prop_map(move |spec| build_project(&spec, levels))
}

/// Characters XML 1.0 forbids – raw or as a character reference.
pub fn xml_problem(xml: &str) -> Option<String> {
    let allowed = |code: u32| matches!(code, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF);
    if let Some((at, character)) = xml
        .char_indices()
        .find(|(_, character)| !allowed(*character as u32))
    {
        return Some(format!("Zeichen {character:?} an Stelle {at}"));
    }
    let mut rest = xml;
    while let Some(at) = rest.find("&#") {
        let reference = &rest[at + 2..];
        let end = reference.find(';')?;
        let code = match reference[..end].strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok(),
            None => reference[..end].parse().ok(),
        };
        if !code.is_some_and(allowed) {
            return Some(format!("Zeichenreferenz &#{};", &reference[..end]));
        }
        rest = &reference[end..];
    }
    None
}
