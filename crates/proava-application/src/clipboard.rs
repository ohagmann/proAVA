//! LV rows and whole LVs in the system clipboard, so that a second
//! proAVA (other window, other project) can paste them completely.
//!
//! The clipboard gets the usual table as plain text and HTML (readable in
//! Excel/Word); the HTML additionally carries one comment with a header
//! line: `proAVA-Zwischenablage|1|<art>|<Zusammenfassung>|<Daten>`. The data
//! is a small proAVA project (the project file format: zip with XML and
//! images) in base64 – texts with formatting, images, position kinds,
//! cost groups, text complements, STLB keys, Ausführungsbeschreibungen.

use proava_domain::transfer::Transfer;
use proava_domain::{ImageAsset, Lv, LvRow, OzMask, Project, ProjectName};

use crate::ports::ProjectRepository;

/// First field of the header line.
pub const MARK: &str = "proAVA-Zwischenablage";
const VERSION: &str = "1";
/// Larger contents are copied without images (clipboard limits, speed).
const MAX_BYTES_WITH_IMAGES: usize = 24 * 1024 * 1024;

/// What lies in the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    /// Rows of an LV (Strg+C in the table).
    Rows,
    /// A whole LV (LV list).
    Lv,
}

impl ClipKind {
    fn key(self) -> &'static str {
        match self {
            ClipKind::Rows => "zeilen",
            ClipKind::Lv => "lv",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "zeilen" => Some(ClipKind::Rows),
            "lv" => Some(ClipKind::Lv),
            _ => None,
        }
    }
}

/// Decoded clipboard content.
#[derive(Debug, Clone)]
pub enum Clip {
    Rows(Transfer),
    Lv {
        lv: Box<Lv>,
        images: Vec<ImageAsset>,
    },
}

/// The header line found in a text (without decoding the data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipHeader {
    pub kind: ClipKind,
    /// „3 Positionen“, „Titel 02 Lüftung mit 14 Positionen“, „LV 2 mit 190 Positionen“.
    pub summary: String,
    pub data: String,
}

impl ClipHeader {
    /// The header line as it goes into the clipboard.
    pub fn line(&self) -> String {
        format!(
            "{MARK}|{VERSION}|{}|{}|{}",
            self.kind.key(),
            clean(&self.summary),
            self.data
        )
    }

    /// Finds a header line in `text` (HTML or plain text); None for foreign text.
    pub fn find(text: &str) -> Option<Self> {
        let start = text.find(MARK)?;
        let rest = &text[start..];
        let end = ["-->", "<", "\n", "\r"]
            .iter()
            .filter_map(|stop| rest.find(stop))
            .min()
            .unwrap_or(rest.len());
        let mut fields = rest[..end].splitn(5, '|');
        let (Some(MARK), Some(VERSION), Some(kind), Some(summary), Some(data)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            return None;
        };
        Some(Self {
            kind: ClipKind::from_key(kind)?,
            summary: summary.to_string(),
            data: data.trim().to_string(),
        })
    }
}

/// No field separators, line breaks or comment ends in the summary.
fn clean(text: &str) -> String {
    text.replace(['|', '\n', '\r', '<', '>'], " ")
        .replace("--", "–")
}

/// „1 Position“, „3 Positionen, 1 Hinweis“, „Titel 02 Lüftung mit 14 Positionen“.
pub fn describe_rows(rows: &[LvRow]) -> String {
    let positions = rows.iter().filter(|row| row.is_position()).count();
    let groups: Vec<_> = rows.iter().filter_map(LvRow::as_group).collect();
    let notes = rows.iter().filter(|row| row.is_note()).count();
    let descriptions = rows.iter().filter(|row| row.is_performance()).count();
    let top = groups.iter().map(|group| group.level()).min();
    let top_titles: Vec<_> = groups
        .iter()
        .filter(|group| Some(group.level()) == top)
        .collect();
    if top_titles.len() == 1
        && rows
            .iter()
            .find(|row| !row.is_performance())
            .is_some_and(LvRow::is_group)
    {
        let title = top_titles[0];
        let name = format!("{} {}", title.ordinal(), title.short_text());
        return format!(
            "Titel {} mit {}",
            name.trim(),
            count(positions, "Position", "Positionen")
        );
    }
    let mut parts = Vec::new();
    if !groups.is_empty() {
        parts.push(count(top_titles.len(), "Titel", "Titel"));
    }
    if positions > 0 || parts.is_empty() {
        parts.push(count(positions, "Position", "Positionen"));
    }
    if notes > 0 {
        parts.push(count(notes, "Hinweis", "Hinweise"));
    }
    if descriptions > 0 {
        parts.push(count(
            descriptions,
            "Ausführungsbeschreibung",
            "Ausführungsbeschreibungen",
        ));
    }
    parts.join(", ")
}

/// „LV 2 Elektro mit 190 Positionen“. A title that already starts with „LV“
/// (an LV without number, titled „LV 480 Gebäudeautomation“) is not
/// prefixed a second time.
pub fn describe_lv(lv: &Lv) -> String {
    let number = lv.number().trim();
    let title = lv.title().trim();
    let title_says_lv = title
        .split_whitespace()
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("LV"));
    let name = match (number, title) {
        ("", "") => "LV".to_string(),
        ("", title) if title_says_lv => title.to_string(),
        ("", title) => format!("LV {title}"),
        (number, "") => format!("LV {number}"),
        (number, title) => format!("LV {number} {title}"),
    };
    format!(
        "{name} mit {}",
        count(lv.position_count(), "Position", "Positionen")
    )
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// Header line for `clip` (rows taken from an LV with the order number mask
/// `mask`); the second value says whether images had to stay behind (too large).
pub fn encode(
    repository: &dyn ProjectRepository,
    clip: &Clip,
    mask: &OzMask,
) -> Result<(ClipHeader, bool), String> {
    let (kind, summary) = match clip {
        Clip::Rows(transfer) => (ClipKind::Rows, describe_rows(&transfer.rows)),
        Clip::Lv { lv, .. } => (ClipKind::Lv, describe_lv(lv)),
    };
    let build = |with_images: bool| -> Result<Vec<u8>, String> {
        let name = ProjectName::parse("Zwischenablage").map_err(|error| error.to_string())?;
        let (lv, images) = match clip {
            Clip::Rows(transfer) => {
                let mut lv = Lv::new("Zwischenablage", mask.clone());
                for row in &transfer.rows {
                    lv.push_row(row.clone());
                }
                (lv, &transfer.images)
            }
            Clip::Lv { lv, images } => ((**lv).clone(), images),
        };
        let mut project = Project::new(name, "", lv);
        if with_images {
            for image in images {
                project.add_asset_copy(image);
            }
        }
        repository.project_bytes(&project)
    };
    let mut bytes = build(true)?;
    let mut without_images = false;
    if bytes.len() > MAX_BYTES_WITH_IMAGES {
        bytes = build(false)?;
        without_images = true;
    }
    Ok((
        ClipHeader {
            kind,
            summary,
            data: base64_encode(&bytes),
        },
        without_images,
    ))
}

/// The content behind a header line.
pub fn decode(repository: &dyn ProjectRepository, header: &ClipHeader) -> Result<Clip, String> {
    let bytes = base64_decode(&header.data)
        .ok_or("Die proAVA-Daten in der Zwischenablage sind beschädigt.")?;
    let project = repository.read_project_bytes(&bytes)?;
    let images = project.images().to_vec();
    let lv = project.lv().clone();
    Ok(match header.kind {
        ClipKind::Rows => Clip::Rows(Transfer {
            rows: lv.rows().to_vec(),
            images,
        }),
        ClipKind::Lv => Clip::Lv {
            lv: Box::new(lv),
            images,
        },
    })
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut values = Vec::with_capacity(text.len());
    for c in text.bytes() {
        match c {
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => values.push(ALPHABET.iter().position(|&a| a == c)? as u32),
        }
    }
    let mut out = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut n = 0u32;
        for (i, value) in chunk.iter().enumerate() {
            n |= value << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        for length in 0..40 {
            let bytes: Vec<u8> = (0..length).map(|i| (i * 37 % 256) as u8).collect();
            let text = base64_encode(&bytes);
            assert_eq!(base64_decode(&text).unwrap(), bytes, "{length}");
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert!(base64_decode("A").is_none());
        assert!(base64_decode("ä").is_none());
    }

    #[test]
    fn header_is_found_in_html_and_foreign_text_is_not() {
        let header = ClipHeader {
            kind: ClipKind::Rows,
            summary: "Titel 02 Lüftung | Klima\nmit 3 Positionen".into(),
            data: "QUJD+/9=".into(),
        };
        let html = format!("<table><tr><td>x</td></tr></table><!--{}-->", header.line());
        let found = ClipHeader::find(&html).unwrap();
        assert_eq!(found.kind, ClipKind::Rows);
        assert_eq!(found.summary, "Titel 02 Lüftung   Klima mit 3 Positionen");
        assert_eq!(found.data, "QUJD+/9=");
        assert!(ClipHeader::find("01.01.0010\tDDC\t1\tSt").is_none());
        assert!(
            ClipHeader::find("proAVA-Zwischenablage|9|zeilen|x|y").is_none(),
            "other version"
        );
    }

    #[test]
    fn lv_summary_says_lv_only_once() {
        let mask = OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap();
        let mut lv = Lv::new("Elektro", mask);
        lv.set_number("2");
        assert_eq!(describe_lv(&lv), "LV 2 Elektro mit 0 Positionen");
        lv.set_number("");
        assert_eq!(describe_lv(&lv), "LV Elektro mit 0 Positionen");
        lv.set_title("LV 480 Gebäudeautomation");
        assert_eq!(
            describe_lv(&lv),
            "LV 480 Gebäudeautomation mit 0 Positionen"
        );
        lv.set_title("lv 480 Gebäudeautomation");
        assert_eq!(
            describe_lv(&lv),
            "lv 480 Gebäudeautomation mit 0 Positionen"
        );
        lv.set_title("LVA Leitsystem");
        assert_eq!(describe_lv(&lv), "LV LVA Leitsystem mit 0 Positionen");
        lv.set_title("");
        lv.set_number("7");
        assert_eq!(describe_lv(&lv), "LV 7 mit 0 Positionen");
    }
}
