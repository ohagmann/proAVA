//! Turns the print model into plain JSON data for the Typst templates.
//! All numbers are formatted here; the templates only place strings.

use rust_decimal::Decimal;
use serde_json::{json, Value};
use typst::foundations::Bytes;
use typst::visualize::{ExchangeFormat, RasterImage, SvgImage};

use crate::format::{bookmark_text, breakable, money, money_in, percent, quantity, round_money};
use crate::model::*;
use crate::options::PrintOptions;

#[derive(Default)]
pub struct Files(Vec<(String, Vec<u8>)>);

impl Files {
    pub fn into_inner(self) -> Vec<(String, Vec<u8>)> {
        self.0
    }

    pub(crate) fn add_image(&mut self, image: &PrintImage) -> Value {
        if let Some(clip) = image.clip {
            return self.add_pdf_clip(image, clip);
        }
        let Some(ext) = image_extension(&image.data) else {
            return json!({ "t": "missing", "name": image.file_name });
        };
        let path = format!("/img/{}.{ext}", self.0.len());
        self.0.push((path.clone(), image.data.clone()));
        json!({
            "t": "img",
            "path": path,
            "width_mm": image.width_mm.filter(|w| *w > 0.0),
            "caption": image.caption,
        })
    }
}

impl Files {
    /// A PDF page cut to a rectangle (vector in the PDF); a broken PDF prints
    /// the „kann nicht dargestellt werden“ note instead of failing the job.
    fn add_pdf_clip(&mut self, image: &PrintImage, clip: PrintClip) -> Value {
        let area = clip.area;
        let usable = crate::pdf_pages(&image.data)
            .ok()
            .and_then(|pages| pages.get(clip.page.max(1) as usize - 1).copied())
            .is_some()
            && [
                area.left,
                area.top,
                area.width,
                area.height,
                clip.page_width,
                clip.page_height,
            ]
            .iter()
            .all(|value| value.is_finite())
            && area.width > 0.5
            && area.height > 0.5;
        if !usable {
            return json!({ "t": "missing", "name": image.file_name });
        }
        let path = format!("/img/{}.pdf", self.0.len());
        self.0.push((path.clone(), image.data.clone()));
        json!({
            "t": "pdfclip",
            "path": path,
            "page": clip.page.max(1),
            "page_width": clip.page_width,
            "page_height": clip.page_height,
            "left": area.left,
            "top": area.top,
            "width": area.width,
            "height": area.height,
            "width_mm": image.width_mm.filter(|w| *w > 0.0),
            "caption": image.caption,
        })
    }
}

fn image_extension(data: &[u8]) -> Option<&'static str> {
    let bytes = Bytes::new(data.to_vec());
    if let Some(format) = ExchangeFormat::detect(data) {
        RasterImage::plain(bytes, format).ok()?;
        return Some(match format {
            ExchangeFormat::Png => "png",
            ExchangeFormat::Jpg => "jpg",
            ExchangeFormat::Gif => "gif",
            ExchangeFormat::Webp => "webp",
        });
    }
    let head = String::from_utf8_lossy(&data[..data.len().min(512)]).to_lowercase();
    if head.contains("<svg") && SvgImage::new(bytes).is_ok() {
        return Some("svg");
    }
    None
}

/// Embedded family for a run (unknown names print with Sans).
pub(crate) fn family(font: Option<&str>) -> &'static str {
    match font {
        Some(FONT_SERIF) => FONT_SERIF,
        Some(FONT_MONO) => FONT_MONO,
        _ => FONT_SANS,
    }
}

fn runs(runs: &[TextRun]) -> Value {
    runs.iter()
        .map(|r| {
            json!({
                // The gap stays one piece (no break chances inside).
                "s": if r.gap { r.text.clone() } else { breakable(&r.text) },
                "gap": r.gap,
                "b": r.bold,
                "i": r.italic,
                "u": r.underline,
                "f": r.font.as_deref().map(|font| family(Some(font))),
                "pt": r.size_pt.filter(|size| size.is_finite() && *size > 0.0),
            })
        })
        .collect()
}

pub(crate) fn blocks(blocks: &[TextBlock], files: &mut Files) -> Value {
    blocks
        .iter()
        .map(|block| match block {
            TextBlock::Paragraph { runs: r, align } => {
                json!({ "t": "p", "runs": runs(r), "align": align.name() })
            }
            TextBlock::ListItem {
                level,
                marker,
                runs: r,
                align,
            } => {
                json!({ "t": "li", "level": level, "marker": marker, "runs": runs(r), "align": align.name() })
            }
            TextBlock::Table {
                header_rows,
                rows,
                widths_mm,
            } => {
                let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
                let empty = TableCell::default();
                let rows: Vec<Value> = rows
                    .iter()
                    .map(|row| {
                        (0..columns)
                            .map(|i| {
                                let cell = row.get(i).unwrap_or(&empty);
                                json!({ "runs": runs(&cell.runs), "align": cell.align.name() })
                            })
                            .collect()
                    })
                    .collect();
                let widths: Vec<Value> = (0..columns)
                    .map(|i| {
                        widths_mm
                            .get(i)
                            .copied()
                            .flatten()
                            .filter(|mm| *mm > 0.0)
                            .into()
                    })
                    .collect();
                json!({
                    "t": "table",
                    "header": (*header_rows).min(rows.len()),
                    "columns": columns,
                    "widths": widths,
                    "rows": rows,
                })
            }
            TextBlock::Image(image) => files.add_image(image),
        })
        .collect()
}

pub(crate) fn header(header: &PrintHeader, default_kind: &str, files: &mut Files) -> Value {
    header_with(header, default_kind, files, &[])
}

/// [`header`] with further placeholder values of this printout, e.g. the free
/// texts of the print dialog („kopftext“, „fusstext“).
pub(crate) fn header_with(
    header: &PrintHeader,
    default_kind: &str,
    files: &mut Files,
    extra: &[(&str, String)],
) -> Value {
    let kind = header.document_kind.as_deref().unwrap_or(default_kind);
    let mut defaults: Vec<(&str, String)> = vec![
        ("projekt", header.project_name.clone()),
        ("projektbezeichnung", header.project_title.clone()),
        ("lv", header.lv_name.clone()),
        ("datum", header.date.clone()),
        ("stand", header.date.clone()),
        ("bearbeiter", header.author.clone()),
        ("ersteller", header.author.clone()),
        ("dokumentart", kind.to_string()),
        // Code of the LV's currency („EUR“, „CHF“) for „netto in {waehrung}“.
        (
            "waehrung",
            match header.currency_sign() {
                "€" => "EUR".to_string(),
                code => code.to_string(),
            },
        ),
    ];
    defaults.extend(extra.iter().cloned());
    json!({
        "frame": crate::frame::frame_json(&header.frame, &defaults),
        "project": breakable(&header.project_name),
        "project_title": breakable(&header.project_title),
        "client": breakable(&header.client),
        "lv": breakable(&header.lv_name),
        "date": header.date,
        "author": header.author,
        // Währung behind amounts and in the column heads.
        "cur": header.currency_sign(),
        "kind": header.document_kind.as_deref().unwrap_or(default_kind),
        "logo": header.logo.as_ref().map(|logo| files.add_image(logo)),
        // Read by templates/common.typ for all colours.
        "look": look_json(&header.look),
    })
}

/// Read by templates/common.typ for all colours and the Blattrahmen.
pub(crate) fn look_json(look: &PrintLook) -> Value {
    json!({
        "style": match look.style {
            PrintStyle::Classic => "classic",
            PrintStyle::ProAva => "proava",
        },
        "accent": look.accent_hex(),
        // Tabellen voll umrandet (common.typ: `grid-on`).
        "grid": look.table_grid,
        "border": match look.border {
            PageBorder::None => "none",
            PageBorder::Thin => "thin",
            PageBorder::Medium => "medium",
        },
        // Formatted texts: factor on the font's line height, gap in pt or „half“.
        "line": f64::from(look.line_spacing_percent.clamp(50, 300)) / 100.0,
        "gap": match look.paragraph_gap {
            crate::ParagraphGap::Points(points) => json!(points),
            crate::ParagraphGap::HalfLine => json!("half"),
        },
    })
}

/// PDF-Lesezeichen for a template (`bookmark` in common.typ): level 1 = top
/// of the outline; null when there is no text.
pub(crate) fn bookmark(level: usize, parts: &[&str]) -> Value {
    let text = bookmark_text(parts);
    if text.is_empty() {
        Value::Null
    } else {
        json!({ "level": level.max(1), "text": text })
    }
}

/// Bookmark of a row in a repeated table header (titles of the LV and the
/// Preisspiegel): `id` tells the layouts apart (`bookmark-once`).
pub(crate) fn bookmark_once(level: usize, parts: &[&str], id: usize) -> Value {
    let mut bm = bookmark(level, parts);
    if let Value::Object(map) = &mut bm {
        map.insert("id".into(), id.into());
    }
    bm
}

/// Heading of a Hinweis or an Ausführungsbeschreibung: the print model
/// carries it as the first paragraph, all bold (to_print).
fn note_heading(blocks: &[TextBlock]) -> Option<String> {
    let Some(TextBlock::Paragraph { runs, .. }) = blocks.first() else {
        return None;
    };
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    let all_bold = runs
        .iter()
        .all(|run| run.bold || run.text.trim().is_empty());
    (all_bold && !text.trim().is_empty()).then(|| text.trim().to_string())
}

fn labels(kinds: &[PositionKind]) -> Vec<&str> {
    kinds.iter().map(PositionKind::label).collect()
}

/// „KG 481“, „LB 070“ of a position (those it has).
fn position_codes(position: &PrintPosition) -> Vec<String> {
    [
        ("KG", &position.cost_group),
        ("LB", &position.work_category),
    ]
    .into_iter()
    .filter_map(|(label, code)| {
        code.as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(|code| format!("{label} {code}"))
    })
    .collect()
}

/// Percentage as printed: „5 %“, „3,5 %“.
fn percent_text(percent: Decimal) -> String {
    let places = percent.normalize().scale().min(6);
    format!("{} %", crate::format::number(percent, places))
}

fn opt_money(value: Option<Decimal>) -> Value {
    value.map(money).into()
}

// ---------- Leistungsverzeichnis ----------

fn has_positions(items: &[PrintItem]) -> bool {
    items.iter().any(|item| match item {
        PrintItem::Title(title) => has_positions(&title.items),
        PrintItem::Position(_) => true,
        _ => false,
    })
}

fn title_total(title: &PrintTitle) -> Decimal {
    title.total.unwrap_or_else(|| items_total(&title.items))
}

fn items_total(items: &[PrintItem]) -> Decimal {
    items
        .iter()
        .map(|item| match item {
            PrintItem::Title(t) => title_total(t),
            PrintItem::Position(p) if p.in_total => p.effective_total().unwrap_or_default(),
            _ => Decimal::ZERO,
        })
        .sum()
}

struct LvBuilder<'a> {
    priced: bool,
    files: &'a mut Files,
    rows: Vec<Value>,
    summary: Vec<Value>,
    /// Rows split at every top-level title (for „neuer Titel auf neuer Seite“).
    chunks: Vec<Vec<Value>>,
    split: bool,
    /// Titles so far (numbers for `bookmark-once`).
    title_count: usize,
    /// PDF-Lesezeichen for every position too (print option; titles always).
    position_bookmarks: bool,
    /// Base font size in pt and Zeilenabstand factor (`keep-with-prices`).
    font_pt: f64,
    line: f64,
    /// A title or position came already: Hinweise before it are Vorbemerkungen.
    started: bool,
    /// The bookmark „Vorbemerkungen“ is set (first Hinweis without heading
    /// before the LV).
    preamble_marked: bool,
    /// Schlussbemerkungen: after the totals (as the planner's AVA prints them).
    closing: Vec<Value>,
}

impl LvBuilder<'_> {
    fn items(&mut self, items: &[PrintItem], level: usize) {
        for item in items {
            match item {
                PrintItem::Title(title) => {
                    let sum = self.priced.then(|| money(title_total(title)));
                    if self.split && level == 1 && !self.rows.is_empty() {
                        let rows = std::mem::take(&mut self.rows);
                        self.chunks.push(rows);
                    }
                    self.started = true;
                    self.title_count += 1;
                    let bm = bookmark_once(level, &[&title.oz, &title.text], self.title_count);
                    self.rows.push(json!({
                        "t": "title", "level": level, "oz": title.oz, "label": title.label.trim(),
                        "text": breakable(&title.text), "bm": bm,
                    }));
                    // A title without positions (only Hinweise) has no sum to fill
                    // in: „Nur Textinformation“.
                    let text_only = !has_positions(&title.items);
                    self.summary.push(json!({
                        "level": level, "oz": title.oz, "label": title.label.trim(),
                        "text": breakable(&title.text), "sum": sum, "text_only": text_only,
                        "lump": title.lump,
                    }));
                    self.items(&title.items, level + 1);
                    self.rows.push(json!({
                        "t": "sum", "level": level, "oz": title.oz, "text": breakable(&title.text), "sum": sum,
                        "text_only": text_only, "lump": title.lump,
                    }));
                }
                PrintItem::Position(p) => {
                    self.started = true;
                    let bm = if self.position_bookmarks {
                        bookmark(level, &[&p.oz, &p.short_text])
                    } else {
                        Value::Null
                    };
                    let (ep, gp) = if self.priced {
                        match &p.surcharge {
                            // Zuschlag: „5 %“ in the EP column
                            Some(surcharge) => (
                                surcharge.percent.map(percent_text).into(),
                                opt_money(p.total_price),
                            ),
                            None => (opt_money(p.unit_price), opt_money(p.effective_total())),
                        }
                    } else {
                        (Value::Null, Value::Null)
                    };
                    // Zuschlag: Bezugssumme in € where the quantity stands; without
                    // prices „%“ to show what the bidder fills in.
                    let (qty, unit) = match &p.surcharge {
                        Some(surcharge) => match surcharge.base.filter(|_| self.priced) {
                            Some(base) => (Value::from(money(base)), "€".to_string()),
                            None => (Value::Null, "%".to_string()),
                        },
                        None => (p.quantity.map(quantity).into(), p.unit.clone()),
                    };
                    // Last block clearly short or tall: the template need not measure it.
                    let keep = crate::keep::keep_with_prices_at(p, self.font_pt, self.line)
                        .map(crate::keep::Keep::name);
                    self.rows.push(json!({
                        "t": "pos",
                        "bm": bm,
                        "oz": p.oz,
                        "short": breakable(&p.short_text),
                        "labels": labels(&p.kinds),
                        "remarks": p.remarks.iter().map(|remark| breakable(remark)).collect::<Vec<_>>(),
                        // „KG 481“, „LB 070“ at the right of the short text.
                        "codes": position_codes(p),
                        "blocks": blocks(&p.long_text, self.files),
                        "keep": keep,
                        // Mengenermittlung (print option): place, Rechenansatz, result.
                        "takeoff": p.takeoff.iter().map(|line| json!({
                            "label": breakable(&line.label),
                            "formula": line.formula,
                            "result": line.result.map(quantity),
                        })).collect::<Vec<_>>(),
                        "takeoff_sum": p.takeoff_sum().map(quantity),
                        "qty": qty,
                        "unit": unit,
                        "ep": ep,
                        "gp": gp,
                        "excluded": !p.in_total,
                        // Pauschal: only the GP is asked for and printed.
                        "lump": p.kinds.contains(&PositionKind::LumpSum),
                        // Running sum for the „Übertrag“ (cents; 0 when not in the sum).
                        "cents": if self.priced && p.in_total {
                            p.effective_total()
                                .map(|total| (round_money(total) * Decimal::ONE_HUNDRED).trunc().to_string().parse::<i64>().unwrap_or(0))
                                .unwrap_or(0)
                        } else {
                            0
                        },
                        "parts": p.price_parts.iter().map(|(label, value)| json!({
                            "label": label,
                            "value": if self.priced { value.map(money) } else { None },
                        })).collect::<Vec<_>>(),
                    }));
                }
                PrintItem::Note(note) | PrintItem::Preliminary(note) => {
                    // Hinweis with a heading under it; the first Hinweis without
                    // one before the LV as „Vorbemerkungen“. A Vorbemerkung
                    // prints without the mark „Hinweis“ over the full width.
                    let bm = match note_heading(note) {
                        Some(heading) => bookmark(level, &[&heading]),
                        None if !self.started && !self.preamble_marked => {
                            self.preamble_marked = true;
                            bookmark(1, &["Vorbemerkungen"])
                        }
                        None => Value::Null,
                    };
                    self.rows.push(json!({
                        "t": "note",
                        "bm": bm,
                        "preliminary": matches!(item, PrintItem::Preliminary(_)),
                        "blocks": blocks(note, self.files),
                    }));
                }
                PrintItem::Closing(note) => {
                    let heading = note_heading(note).unwrap_or_else(|| "Schlussbemerkung".into());
                    self.closing.push(json!({
                        "bm": bookmark(1, &[&heading]),
                        "blocks": blocks(note, self.files),
                    }));
                }
                PrintItem::Performance(number, text) => {
                    let heading = note_heading(text).unwrap_or_default();
                    self.rows.push(json!({
                        "t": "note",
                        "bm": bookmark(level, &[number, &heading]),
                        "performance": number,
                        "blocks": blocks(text, self.files),
                    }));
                }
            }
        }
    }
}

pub fn lv(lv: &PrintLv, mode: LvPrintMode, options: &PrintOptions, files: &mut Files) -> Value {
    let priced = mode == LvPrintMode::Priced;
    let default_kind = match mode {
        LvPrintMode::Blank => "Angebotsaufforderung",
        // DIN 276, 4.3.5: the LV with the planner's prices.
        LvPrintMode::Priced => "Kostenvoranschlag",
    };
    let head = header_with(
        &lv.header,
        default_kind,
        files,
        &[
            ("kopftext", options.header_text.trim().to_string()),
            ("fusstext", options.footer_text.trim().to_string()),
        ],
    );
    let mut builder = LvBuilder {
        priced,
        files,
        rows: Vec::new(),
        summary: Vec::new(),
        chunks: Vec::new(),
        split: options.title_page_break,
        title_count: 0,
        position_bookmarks: options.bookmark_positions,
        font_pt: f64::from(options.font_size()),
        line: f64::from(lv.header.look.line_spacing_percent.clamp(50, 300)) / 100.0,
        started: false,
        preamble_marked: false,
        closing: Vec::new(),
    };
    builder.items(&lv.items, 1);
    let rest = std::mem::take(&mut builder.rows);
    if !rest.is_empty() || builder.chunks.is_empty() {
        builder.chunks.push(rest);
    }

    let net = lv.net_total.unwrap_or_else(|| items_total(&lv.items));
    let discount = lv
        .discount
        .as_ref()
        .map(|(_, amount)| *amount)
        .unwrap_or_default();
    let after = net - discount;
    let vat = round_money(after * lv.vat_percent / Decimal::ONE_HUNDRED);
    let cur = lv.header.currency_sign();
    let totals = priced.then(|| {
        json!({
            "net": money_in(net, cur),
            "lump": lv.lump_sum.map(|lump| money_in(lump, cur)),
            "positions": money_in(items_total(&lv.items), cur),
            "discount": lv.discount.as_ref().map(|(label, amount)| json!({
                "label": label,
                "amount": money_in(-*amount, cur),
                "after": money_in(after, cur),
            })),
            "vat": money_in(vat, cur),
            "gross": money_in(after + vat, cur),
        })
    });
    json!({
        "header": head,
        "priced": priced,
        "chunks": builder.chunks,
        "summary": builder.summary,
        "closing": builder.closing,
        "options": json!({
            "landscape": options.landscape,
            "font_size": options.font_size(),
            "title_break": options.title_page_break,
            "summary": options.summary || options.summary_only,
            "summary_only": options.summary_only,
            "header_text": options.header_text.trim(),
            "footer_text": options.footer_text.trim(),
            "contents": options.contents,
            "carry": options.carry,
        }),
        "vat_label": format!("zzgl. MwSt. {}", percent(lv.vat_percent)),
        "totals": totals,
        "parties": lv.parties.iter().map(|(label, text)| json!({
            "label": label,
            "text": text,
        })).collect::<Vec<_>>(),
        "cover": lv.cover.as_ref().map(|cover| cover_json(cover, priced, lv.vat_percent, &lv.header)),
    })
}

/// Deckblatt for `templates/cover.typ`.
fn cover_json(
    cover: &PrintCover,
    priced: bool,
    vat_percent: Decimal,
    header: &PrintHeader,
) -> Value {
    let date = header.date.as_str();
    let lines = |text: &str| text.lines().map(breakable).collect::<Vec<_>>();
    let mut facts: serde_json::Map<String, Value> = cover
        .facts
        .iter()
        .filter(|(_, value)| !value.trim().is_empty())
        .map(|(key, value)| (key.clone(), Value::String(value.trim().to_string())))
        .collect();
    // „Stand“ = date of the printout unless the cover brings its own.
    if !date.trim().is_empty() {
        facts
            .entry("stand")
            .or_insert_with(|| Value::String(date.trim().to_string()));
    }
    let addresses: serde_json::Map<String, Value> = cover
        .addresses
        .iter()
        .map(|(key, lines)| {
            let lines: Vec<String> = lines
                .iter()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .map(breakable)
                .collect();
            (key.clone(), json!(lines))
        })
        .filter(|(_, lines)| lines.as_array().is_some_and(|lines| !lines.is_empty()))
        .collect();
    json!({
        "style": match cover.style {
            CoverStyle::Standard => "standard",
            CoverStyle::Letterhead => "letterhead",
        },
        "blocks": {
            "logo": cover.blocks.logo,
            "addresses": cover.blocks.addresses,
            "facts": cover.blocks.facts,
        },
        // The bidder fills in only the Angebotsaufforderung.
        "bidder": cover.blocks.bidder && !priced,
        "vat_label": format!("zzgl. USt {}", percent(vat_percent)),
        "cur": header.currency_sign(),
        "lv": breakable(cover.lv.trim()),
        "project": breakable(cover.project.trim()),
        "measure": cover.measure.trim(),
        "facts": facts,
        "addresses": addresses,
        "sections": cover.sections.iter().filter(|section| !section.rows.is_empty()).map(|section| json!({
            "heading": section.heading,
            "rows": section.rows.iter().map(|(label, text)| json!({
                "label": label,
                "lines": lines(text),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "notes": cover.notes.iter().map(|(label, text)| json!({
            "label": label,
            "lines": lines(text),
        })).collect::<Vec<_>>(),
    })
}

// ---------- Preisspiegel ----------

/// "low"/"high" for the cheapest/most expensive value, "missing" where a bidder
/// left a price open that others filled in.
fn marks(values: &[Option<Decimal>]) -> Vec<&'static str> {
    let present: Vec<Decimal> = values.iter().flatten().copied().collect();
    let min = present.iter().min();
    let max = present.iter().max();
    let compare = present.len() >= 2 && min != max;
    values
        .iter()
        .map(|v| match v {
            None if !present.is_empty() => "missing",
            Some(v) if compare && Some(v) == min => "low",
            Some(v) if compare && Some(v) == max => "high",
            _ => "",
        })
        .collect()
}

fn cells(values: &[Option<Decimal>]) -> Vec<Value> {
    values
        .iter()
        .zip(marks(values))
        .map(|(v, m)| json!({ "v": opt_money(*v), "m": m }))
        .collect()
}

fn mean(values: &[Option<Decimal>]) -> Option<Decimal> {
    let present: Vec<Decimal> = values.iter().flatten().copied().collect();
    if present.is_empty() {
        return None;
    }
    Some(round_money(
        present.iter().sum::<Decimal>() / Decimal::from(present.len()),
    ))
}

fn minimum(values: &[Option<Decimal>]) -> Option<Decimal> {
    values.iter().flatten().min().copied()
}

fn bid_total(position: &PcPosition, bidder: usize) -> Option<Decimal> {
    let price = position.prices.get(bidder)?;
    price
        .total_price
        .or_else(|| Some(round_money(position.quantity? * price.unit_price?)))
}

fn pc_title_totals(title: &PcTitle, bidders: usize) -> Vec<Option<Decimal>> {
    let computed = pc_items_totals(&title.items, bidders);
    (0..bidders)
        .map(|b| title.totals.get(b).copied().flatten().or(computed[b]))
        .collect()
}

fn pc_items_totals(items: &[PcItem], bidders: usize) -> Vec<Option<Decimal>> {
    let mut totals = vec![None::<Decimal>; bidders];
    for item in items {
        let values = match item {
            PcItem::Title(t) => pc_title_totals(t, bidders),
            PcItem::Position(p) if p.in_total => (0..bidders).map(|b| bid_total(p, b)).collect(),
            PcItem::Position(_) => continue,
        };
        for (total, value) in totals.iter_mut().zip(values) {
            if let Some(value) = value {
                *total = Some(total.unwrap_or_default() + value);
            }
        }
    }
    totals
}

/// Percent above the cheapest value of the line, e.g. „+12,5“; None for the
/// cheapest, missing values and lines without two prices.
fn above_lowest(values: &[Option<Decimal>]) -> Vec<Option<String>> {
    let lowest = minimum(values).filter(|lowest| *lowest > Decimal::ZERO);
    let priced = values.iter().flatten().count();
    values
        .iter()
        .map(|value| {
            let (value, lowest) = ((*value)?, lowest?);
            (priced >= 2 && value > lowest).then(|| {
                format!(
                    "+{}",
                    crate::format::number((value - lowest) * Decimal::ONE_HUNDRED / lowest, 1)
                )
            })
        })
        .collect()
}

/// [`cells`] with a flag per value („?“ Ausreißer, „!“ Rechenfehler).
fn flagged_cells(values: &[Option<Decimal>], flags: &[&str]) -> Vec<Value> {
    let mut cells = cells(values);
    for (cell, flag) in cells.iter_mut().zip(flags) {
        cell["f"] = json!(flag);
    }
    cells
}

fn pc_rows(items: &[PcItem], bidders: usize, level: usize, rows: &mut Vec<Value>) {
    for item in items {
        match item {
            PcItem::Title(title) => {
                let bm = bookmark_once(level, &[&title.oz, &title.text], rows.len());
                rows.push(json!({
                    "t": "title", "level": level, "oz": title.oz, "text": breakable(&title.text), "bm": bm,
                }));
                pc_rows(&title.items, bidders, level + 1, rows);
                let sums = pc_title_totals(title, bidders);
                rows.push(json!({
                    "t": "sum", "level": level, "oz": title.oz, "text": breakable(&title.text),
                    "sums": cells(&sums),
                    "pct": above_lowest(&sums),
                    "mean": opt_money(mean(&sums)),
                    "min": opt_money(minimum(&sums)),
                    "est": opt_money(title.estimate),
                }));
            }
            PcItem::Position(p) => {
                let eps: Vec<Option<Decimal>> = (0..bidders)
                    .map(|b| p.prices.get(b).and_then(|price| price.unit_price))
                    .collect();
                let gps: Vec<Option<Decimal>> = (0..bidders).map(|b| bid_total(p, b)).collect();
                let flag = |b: usize, pick: fn(&BidPrice) -> bool, mark: &'static str| {
                    p.prices
                        .get(b)
                        .filter(|price| pick(price))
                        .map(|_| mark)
                        .unwrap_or("")
                };
                let outliers: Vec<&str> = (0..bidders)
                    .map(|b| flag(b, |price| price.outlier, "?"))
                    .collect();
                let errors: Vec<&str> = (0..bidders)
                    .map(|b| flag(b, |price| price.error, "!"))
                    .collect();
                rows.push(json!({
                    "t": "pos",
                    "oz": p.oz,
                    "short": breakable(&p.short_text),
                    "labels": labels(&p.kinds),
                    "qty": p.quantity.map(quantity),
                    "unit": p.unit,
                    "ep": flagged_cells(&eps, &outliers),
                    "gp": flagged_cells(&gps, &errors),
                    "pct": above_lowest(&gps),
                    "est_ep": opt_money(p.estimate.unit_price),
                    "est_gp": opt_money(p.estimate.total_price.or_else(|| {
                        Some(round_money(p.estimate.unit_price? * p.quantity?))
                    })),
                    "mean_ep": opt_money(mean(&eps)),
                    "mean_gp": opt_money(mean(&gps)),
                    "min_ep": opt_money(minimum(&eps)),
                    "min_gp": opt_money(minimum(&gps)),
                    "excluded": !p.in_total,
                    "qty_note": p.quantity_note,
                }));
            }
        }
    }
}

pub fn price_comparison(pc: &PrintPriceComparison, files: &mut Files) -> Value {
    let count = pc.bidders.len();
    let mut rows = Vec::new();
    pc_rows(&pc.items, count, 1, &mut rows);

    let computed = pc_items_totals(&pc.items, count);
    let nets: Vec<Option<Decimal>> = pc
        .bidders
        .iter()
        .zip(&computed)
        .map(|(b, c)| b.net_total.or(*c))
        .collect();
    let discounts: Vec<Option<Decimal>> = pc
        .bidders
        .iter()
        .zip(&nets)
        .map(|(b, net)| {
            b.discount_amount.or_else(|| {
                Some(round_money(
                    (*net)? * b.discount_percent? / Decimal::ONE_HUNDRED,
                ))
            })
        })
        .collect();
    let after: Vec<Option<Decimal>> = nets
        .iter()
        .zip(&discounts)
        .map(|(net, d)| net.map(|n| n - d.unwrap_or_default()))
        .collect();
    let vats: Vec<Option<Decimal>> = after
        .iter()
        .map(|a| a.map(|a| round_money(a * pc.vat_percent / Decimal::ONE_HUNDRED)))
        .collect();
    let grosses: Vec<Option<Decimal>> = after
        .iter()
        .zip(&vats)
        .map(|(a, v)| Some((*a)? + (*v)?))
        .collect();
    let estimate_net = pc.estimate_net;
    let estimate_vat =
        estimate_net.map(|net| round_money(net * pc.vat_percent / Decimal::ONE_HUNDRED));
    let cash_discounts: Vec<Value> = grosses
        .iter()
        .map(|gross| {
            let (gross, percent) = ((*gross)?, pc.cash_discount_percent?);
            Some(money(round_money(gross * percent / Decimal::ONE_HUNDRED)))
        })
        .map(Value::from)
        .collect();
    let ranks: Vec<Value> = after
        .iter()
        .map(|a| match a {
            Some(a) => json!(1 + after.iter().flatten().filter(|o| *o < a).count()),
            None => Value::Null,
        })
        .collect();
    let discount_labels: Vec<Value> = pc
        .bidders
        .iter()
        .map(|b| match (b.discount_amount, b.discount_percent) {
            (Some(_), _) => json!("Betrag"),
            (None, Some(p)) => json!(percent(p)),
            (None, None) => Value::Null,
        })
        .collect();
    // Net after discount with the current LV quantities (Massenmehrung/
    // -minderung after the bids came in), only when some bidder has it.
    let at_lv: Vec<Option<Decimal>> = pc
        .bidders
        .iter()
        .map(|bidder| bidder.after_discount_at_lv_quantities)
        .collect();
    let bidders: Vec<Value> = pc
        .bidders
        .iter()
        .enumerate()
        .map(|(i, b)| {
            json!({
                "name": breakable(&b.name),
                "note": breakable(&b.note),
                "date": b.date,
                // Netto nach Nachlass: what the rank compares.
                "net": after[i].map(|net| money_in(net, pc.header.currency_sign())),
                "gross": grosses[i].map(|gross| money_in(gross, pc.header.currency_sign())),
                "rank": ranks[i],
            })
        })
        .collect();
    let a3 = match pc.paper {
        PaperSize::Auto => count > 3,
        PaperSize::A4 => false,
        PaperSize::A3 => true,
    };

    json!({
        "header": header(&pc.header, "Preisspiegel", files),
        "a3": a3,
        "bidders": bidders,
        "rows": rows,
        "totals": {
            "net": cells(&nets),
            "mean_net": opt_money(mean(&nets)),
            "min_net": opt_money(minimum(&nets)),
            "discount_label": discount_labels,
            "discount": discounts.iter().map(|d| opt_money(*d)).collect::<Vec<_>>(),
            "after": cells(&after),
            "vat": vats.iter().map(|v| opt_money(*v)).collect::<Vec<_>>(),
            "gross": cells(&grosses),
            "rank": ranks,
            "at_lv": if at_lv.iter().any(Option::is_some) { json!(cells(&at_lv)) } else { Value::Null },
            "mean_after": opt_money(mean(&after)),
            "min_after": opt_money(minimum(&after)),
            "mean_gross": opt_money(mean(&grosses)),
            "min_gross": opt_money(minimum(&grosses)),
            // „% im Vergleich“: above the cheapest after discount.
            "pct": above_lowest(&after),
            "est_net": opt_money(estimate_net),
            "est_vat": opt_money(estimate_vat),
            "est_gross": opt_money(estimate_net.zip(estimate_vat).map(|(net, vat)| net + vat)),
            "cash_discount": cash_discounts,
        },
        "estimate": pc.estimate,
        "net_label": if discounts.iter().flatten().any(|discount| !discount.is_zero()) {
            "netto n. Nachlass"
        } else {
            "netto"
        },
        "amounts_note": format!(
            "Alle Einzelbeträge netto in {}",
            match pc.header.currency_sign() {
                "€" => "EUR",
                sign => sign,
            }
        ),
        "bid_notes": bid_notes(pc),
        "appendix": structure_appendix(pc),
        "outlier_note": pc.outlier_note,
        "cash_discount_label": pc.cash_discount_percent.map(|value| {
            let note = pc.cash_discount_note.trim();
            format!("Skonto {} auf brutto{} – informativ, nicht gewertet", percent(value), if note.is_empty() { String::new() } else { format!(" ({note})") })
        }),
        "vat_label": format!("MwSt. {}", percent(pc.vat_percent)),
    })
}

/// How many order numbers or Rechenfehler a note lists by name.
const NOTE_LIST: usize = 8;

/// „a, b, c“ – at most [`NOTE_LIST`], then „… und 4 weitere“.
fn note_list(items: Vec<String>, separator: &str) -> String {
    let more = items.len().saturating_sub(NOTE_LIST);
    let mut text = items
        .into_iter()
        .take(NOTE_LIST)
        .collect::<Vec<_>>()
        .join(separator);
    if more > 0 {
        text.push_str(&format!(" und {more} weitere"));
    }
    text
}

/// More deviations of one bid than this go to the appendix.
const STRUCTURE_INLINE: usize = 8;

/// Bids with many deviations: every line on the last pages.
fn structure_appendix(pc: &PrintPriceComparison) -> Value {
    let list: Vec<Value> = pc
        .bidders
        .iter()
        .filter_map(|bidder| {
            let structure = bidder.structure.as_ref()?;
            (structure.lines.len() > STRUCTURE_INLINE).then(|| {
                json!({
                    "name": breakable(&bidder.name),
                    "summary": structure.summary,
                    "lines": structure.lines,
                })
            })
        })
        .collect();
    Value::Array(list)
}

/// Notes under the Angebotssummen: order numbers of a bid that are not in
/// the LV (not ranked) and Rechenfehler (ranked with quantity × EP) – the
/// sums stay as they are, the reader learns why.
fn bid_notes(pc: &PrintPriceComparison) -> Vec<String> {
    let mut notes = Vec::new();
    for bidder in &pc.bidders {
        // Strukturabweichungen (Anwender 20.11.): short, the list inline or
        // in the appendix.
        if let Some(structure) = &bidder.structure {
            let mut note = format!("Angebot {}: {}", bidder.name, structure.summary);
            if let Some(sum) = &structure.sum_note {
                note.push_str(&format!("; {sum}"));
            }
            if structure.lines.len() > STRUCTURE_INLINE {
                note.push_str(&format!(
                    " – Einzelliste ({} Einträge) im Anhang.",
                    structure.lines.len()
                ));
            } else {
                note.push_str(&format!(" – {}.", structure.lines.join("; ")));
            }
            notes.push(note);
        }
        let unknown = bidder.unknown_ordinals.len();
        if unknown > 0 {
            let (positions, stand, are) = if unknown == 1 {
                ("1 Position".to_string(), "steht", "sie ist")
            } else {
                (format!("{unknown} Positionen"), "stehen", "sie sind")
            };
            notes.push(format!(
                "Angebot {} enthält {positions}, die nicht im LV {stand} (OZ {}); {are} nicht gewertet.",
                bidder.name,
                note_list(bidder.unknown_ordinals.clone(), ", ")
            ));
        }
        let errors = bidder.arithmetic_errors.len();
        if errors > 0 {
            notes.push(format!(
                "Angebot {}: {errors} Rechenfehler, gewertet mit Menge × EP ({}).",
                bidder.name,
                note_list(
                    bidder
                        .arithmetic_errors
                        .iter()
                        .map(|error| format!(
                            "OZ {}: GP im Angebot {}, Menge × EP {}",
                            error.oz,
                            money(error.stated),
                            money(error.computed)
                        ))
                        .collect(),
                    "; "
                )
            ));
        }
    }
    notes
}

/// Data for `cost_groups.typ`.
pub fn cost_groups(report: &PrintCostGroups, files: &mut Files) -> Value {
    let share = |value: Decimal, total: Decimal| -> String {
        if total.is_zero() {
            String::new()
        } else {
            format!(
                "{} %",
                crate::format::number(value * Decimal::ONE_HUNDRED / total, 1)
            )
        }
    };
    json!({
        "header": header(&report.header, "Kosten nach Kostengruppen", files),
        "catalog": report.catalog,
        "bidders": report.bidders,
        "landscape": report.bidders.len() > 2,
        "lines": report.lines.iter().map(|line| json!({
            "code": line.code,
            "label": line.label,
            "subtotal": line.subtotal,
            "positions": line.positions,
            "planner": money(line.planner),
            "share": share(line.planner, report.planner_total),
            "bidders": line.bidders.iter().map(|value| money(*value)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "planner_total": money(report.planner_total),
        "bidder_totals": report.bidder_totals.iter().map(|value| money(*value)).collect::<Vec<_>>(),
    })
}

// ---------- Änderungsliste ----------

pub fn change_list(list: &PrintChangeList, files: &mut Files) -> Value {
    json!({
        "header": header(&list.header, "Änderungsliste", files),
        "landscape": list.landscape,
        "against": breakable(&list.against),
        "summary": list.summary,
        "entries": list.entries.iter().map(|entry| json!({
            // PDF-Lesezeichen: „01.0010 Kurztext (geändert)“.
            "bm": bookmark(1, &[
                &entry.ordinal,
                &entry.text,
                &if entry.kind.trim().is_empty() { String::new() } else { format!("({})", entry.kind.trim()) },
            ]),
            "kind": entry.kind,
            "row_kind": entry.row_kind,
            "ordinal": breakable(&entry.ordinal),
            "old_ordinal": breakable(&entry.old_ordinal),
            "text": breakable(&entry.text),
            "details": entry.details.iter().map(|detail| breakable(detail)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}
