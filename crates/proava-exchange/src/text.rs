use proava_domain::{
    normalize_runs, Address, Align, Cell, ImageAsset, RichText, Run, Table, TextBlock,
};
use proava_gaeb as gaeb;

/// Domain long text → GAEB text with the images of the project (the GAEB
/// writers leave them out unless asked); returns the number of images.
pub(crate) fn rich_to_gaeb(text: &RichText, assets: &[ImageAsset]) -> (gaeb::RichText, usize) {
    let mut images = 0;
    let blocks = text
        .blocks()
        .iter()
        .filter_map(|block| match block {
            TextBlock::Paragraph { runs, align } => Some(gaeb::TextBlock::Paragraph {
                runs: runs_to_gaeb(runs),
                align: align_to_gaeb(*align),
            }),
            TextBlock::ListItem {
                ordered,
                level,
                runs,
                align,
            } => Some(gaeb::TextBlock::ListItem {
                ordered: *ordered,
                level: *level,
                runs: runs_to_gaeb(runs),
                align: align_to_gaeb(*align),
            }),
            TextBlock::Table(table) => Some(gaeb::TextBlock::Table(gaeb::Table {
                rows: table
                    .rows
                    .iter()
                    .map(|cells| {
                        cells
                            .iter()
                            .map(|cell| gaeb::Cell {
                                runs: runs_to_gaeb(&cell.runs),
                                align: align_to_gaeb(cell.align),
                            })
                            .collect()
                    })
                    .collect(),
                header_rows: table.header_rows,
                column_widths_mm: table.column_widths_mm.clone(),
            })),
            TextBlock::Image { file, width_mm } => {
                images += 1;
                let asset = assets.iter().find(|asset| asset.file == *file)?;
                if asset.clip.is_some() {
                    // GAEB only knows pixels: the clip as PNG at 300 dpi.
                    let width =
                        width_mm.unwrap_or(proava_application::pdf_clip::DEFAULT_CLIP_WIDTH_MM);
                    let picture = crate::clip::gaeb_picture(asset, width).ok()?;
                    let stem = asset.file.trim_end_matches(".pdf");
                    return Some(gaeb::TextBlock::Image {
                        file_name: format!("{stem}.png"),
                        data: (*picture.png).clone(),
                        width_mm: Some(width),
                    });
                }
                Some(gaeb::TextBlock::Image {
                    file_name: asset.file.clone(),
                    data: asset.bytes.clone(),
                    width_mm: *width_mm,
                })
            }
        })
        .collect();
    (gaeb::RichText { blocks }, images)
}

fn align_to_gaeb(align: Align) -> gaeb::Align {
    match align {
        Align::Left => gaeb::Align::Left,
        Align::Center => gaeb::Align::Center,
        Align::Right => gaeb::Align::Right,
        Align::Justify => gaeb::Align::Justify,
    }
}

fn align_from_gaeb(align: gaeb::Align) -> Align {
    match align {
        gaeb::Align::Left => Align::Left,
        gaeb::Align::Center => Align::Center,
        gaeb::Align::Right => Align::Right,
        gaeb::Align::Justify => Align::Justify,
    }
}

/// Images found in GAEB texts, with the names they get in the project
/// (cleaned and made unique as `Project::add_image` does; the same picture
/// twice keeps one name).
#[derive(Debug, Default)]
pub(crate) struct ImportedImages {
    pub images: Vec<(String, Vec<u8>)>,
}

impl ImportedImages {
    fn add(&mut self, wanted: &str, data: &[u8]) -> String {
        if let Some((name, _)) = self.images.iter().find(|(_, bytes)| bytes == data) {
            return name.clone();
        }
        let base = wanted.rsplit(['/', '\\']).next().unwrap_or(wanted);
        let mut cleaned: String = base
            .chars()
            .map(|ch| {
                if ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        if cleaned.trim_matches(['.', '_']).is_empty() {
            cleaned = "bild.png".to_string();
        }
        let (stem, extension) = match cleaned.rsplit_once('.') {
            Some((stem, extension)) => (stem.to_string(), format!(".{extension}")),
            None => (cleaned.clone(), String::new()),
        };
        let mut name = cleaned;
        let mut counter = 2;
        while self.images.iter().any(|(taken, _)| *taken == name) {
            name = format!("{stem}_{counter}{extension}");
            counter += 1;
        }
        self.images.push((name.clone(), data.to_vec()));
        name
    }
}

pub(crate) fn rich_from_gaeb(text: &gaeb::RichText, images: &mut ImportedImages) -> RichText {
    RichText::new(
        text.blocks
            .iter()
            .map(|block| match block {
                gaeb::TextBlock::Paragraph { runs, align } => TextBlock::Paragraph {
                    runs: runs_from_gaeb(runs),
                    align: align_from_gaeb(*align),
                },
                gaeb::TextBlock::ListItem {
                    ordered,
                    level,
                    runs,
                    align,
                } => TextBlock::ListItem {
                    ordered: *ordered,
                    level: (*level).min(proava_domain::MAX_LIST_LEVEL),
                    runs: runs_from_gaeb(runs),
                    align: align_from_gaeb(*align),
                },
                gaeb::TextBlock::Table(table) => TextBlock::Table(Table {
                    rows: table
                        .rows
                        .iter()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|cell| Cell {
                                    runs: runs_from_gaeb(&cell.runs),
                                    align: align_from_gaeb(cell.align),
                                })
                                .collect()
                        })
                        .collect(),
                    header_rows: table.header_rows,
                    column_widths_mm: table.column_widths_mm.clone(),
                }),
                gaeb::TextBlock::Image {
                    file_name,
                    data,
                    width_mm,
                } => TextBlock::Image {
                    file: images.add(file_name, data),
                    width_mm: *width_mm,
                },
            })
            .collect(),
    )
}

fn runs_to_gaeb(runs: &[Run]) -> Vec<gaeb::Run> {
    runs.iter()
        .map(|run| gaeb::Run {
            text: run.text.clone(),
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            font: run.font.clone(),
            size_pt: run.size_pt,
        })
        .collect()
}

/// Imported runs keep the font name as written; sizes equal to the standard
/// 10 pt and the usual standard families are dropped so that ordinary texts
/// stay unformatted.
fn runs_from_gaeb(runs: &[gaeb::Run]) -> Vec<Run> {
    let runs = runs
        .iter()
        .map(|run| Run {
            text: run.text.clone(),
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            font: run.font.clone().filter(|font| !is_standard_font(font)),
            size_pt: run
                .size_pt
                .filter(|size| (size - proava_domain::DEFAULT_SIZE_PT).abs() > 0.05),
        })
        .collect();
    normalize_runs(runs)
}

/// Arial and its stand-ins are what proAVA shows anyway.
fn is_standard_font(font: &str) -> bool {
    matches!(
        font.trim().to_ascii_lowercase().as_str(),
        "arial" | "helvetica" | "helv" | "liberation sans" | "arimo" | "sans-serif"
    )
}

pub(crate) fn address_to_gaeb(address: &Address) -> gaeb::Address {
    gaeb::Address {
        name1: address.name1.clone(),
        name2: address.name2.clone(),
        name3: address.name3.clone(),
        name4: address.name4.clone(),
        street: address.street.clone(),
        postal_code: address.postal_code.clone(),
        city: address.city.clone(),
        country: address.country.clone(),
        iln: address.iln.clone(),
        contact: address.contact.clone(),
        phone: address.phone.clone(),
        fax: address.fax.clone(),
        email: address.email.clone(),
        vat_id: address.vat_id.clone(),
    }
}

pub(crate) fn address_from_gaeb(address: &gaeb::Address) -> Address {
    Address {
        name1: address.name1.clone(),
        name2: address.name2.clone(),
        name3: address.name3.clone(),
        name4: address.name4.clone(),
        street: address.street.clone(),
        postal_code: address.postal_code.clone(),
        city: address.city.clone(),
        country: address.country.clone(),
        iln: address.iln.clone(),
        contact: address.contact.clone(),
        phone: address.phone.clone(),
        fax: address.fax.clone(),
        email: address.email.clone(),
        vat_id: address.vat_id.clone(),
    }
}

pub(crate) fn is_empty_address(address: &Address) -> bool {
    *address == Address::default()
}

/// A place of the award (Requester, CnstSite, NotifSite) for GAEB.
pub(crate) fn place_to_gaeb(place: &proava_domain::Place) -> gaeb::Place {
    gaeb::Place {
        address: (!is_empty_address(&place.address)).then(|| address_to_gaeb(&place.address)),
        id_no: place.number.trim().to_string(),
        name: place.name.trim().to_string(),
    }
}

pub(crate) fn place_from_gaeb(place: &gaeb::Place) -> proava_domain::Place {
    proava_domain::Place {
        address: place
            .address
            .as_ref()
            .map(address_from_gaeb)
            .unwrap_or_default(),
        number: place.id_no.trim().to_string(),
        name: place.name.trim().to_string(),
    }
}

/// Requester, CnstSite/NotifSite and the OWN fields of `info` into `award`.
pub(crate) fn places_to_gaeb(info: &proava_domain::ProjectInfo, award: &mut gaeb::Award) {
    award.requesters = info.requesters.iter().map(place_to_gaeb).collect();
    let first = proava_domain::Place {
        address: info.site.clone(),
        number: info.site_number.clone(),
        name: info.site_name.clone(),
    };
    award.sites.clear();
    if !first.is_empty() || info.notification_site.is_some() {
        award.sites.push(gaeb::Site {
            site: place_to_gaeb(&first),
            notification: info.notification_site.as_ref().map(place_to_gaeb),
        });
    }
    award
        .sites
        .extend(info.further_sites.iter().map(|site| gaeb::Site {
            site: place_to_gaeb(&site.site),
            notification: site.notification.as_ref().map(place_to_gaeb),
        }));
    award.owner_account = info.client_account.trim().to_string();
    award.owner_country = gaeb::Country {
        kind: info.client_country_kind.trim().to_string(),
        name: info.client_country_code.trim().to_string(),
    };
}

/// The other way: what a GAEB award says about places and the OWN fields.
pub(crate) fn places_from_gaeb(award: &gaeb::Award, info: &mut proava_domain::ProjectInfo) {
    info.requesters = award.requesters.iter().map(place_from_gaeb).collect();
    let mut sites = award.sites.iter();
    if let Some(first) = sites.next() {
        let place = place_from_gaeb(&first.site);
        info.site = place.address;
        info.site_number = place.number;
        info.site_name = place.name;
        info.notification_site = first.notification.as_ref().map(place_from_gaeb);
    }
    info.further_sites = sites
        .map(|site| proava_domain::ExecutionSite {
            site: place_from_gaeb(&site.site),
            notification: site.notification.as_ref().map(place_from_gaeb),
        })
        .collect();
    info.client_account = award.owner_account.trim().to_string();
    info.client_country_kind = award.owner_country.kind.trim().to_string();
    info.client_country_code = award.owner_country.name.trim().to_string();
}

/// "15.12.2026, 10:00 Uhr" → ("2026-12-15", "10:00:00"); also ISO dates.
pub(crate) fn parse_deadline(text: &str) -> (Option<String>, Option<String>) {
    let mut date = None;
    let mut time = None;
    for token in text
        .split([' ', ',', ';'])
        .filter(|token| !token.is_empty())
    {
        let token = token.trim_end_matches('.');
        if date.is_none() {
            date = parse_date(token);
            if date.is_some() {
                continue;
            }
        }
        if time.is_none() {
            time = parse_time(token);
        }
    }
    (date, time)
}

fn parse_date(token: &str) -> Option<String> {
    let numbers: Vec<&str> = token.split(['.', '-']).collect();
    if numbers.len() != 3
        || !numbers
            .iter()
            .all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let (day, month, year) = if token.contains('-') {
        (numbers[2], numbers[1], numbers[0])
    } else {
        (numbers[0], numbers[1], numbers[2])
    };
    let (day, month, year): (u32, u32, u32) =
        (day.parse().ok()?, month.parse().ok()?, year.parse().ok()?);
    let year = if year < 100 { 2000 + year } else { year };
    ((1..=31).contains(&day) && (1..=12).contains(&month))
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}

fn parse_time(token: &str) -> Option<String> {
    let numbers: Vec<&str> = token.split(':').collect();
    if numbers.len() < 2 || numbers.len() > 3 {
        return None;
    }
    let values: Vec<u32> = numbers
        .iter()
        .map(|n| n.parse().ok())
        .collect::<Option<_>>()?;
    let second = values.get(2).copied().unwrap_or(0);
    (values[0] < 24 && values[1] < 60 && second < 60)
        .then(|| format!("{:02}:{:02}:{:02}", values[0], values[1], second))
}

/// GAEB award procedures (AwardInfo/Cat) and how the user writes them.
const PROCEDURES: [(&str, &str); 11] = [
    ("OpenProc", "Offenes Verfahren"),
    ("ClosedProc", "Nicht offenes Verfahren"),
    ("NegProc", "Verhandlungsverfahren"),
    ("OpenCall", "Öffentliche Ausschreibung"),
    ("SelectCall", "Beschränkte Ausschreibung"),
    ("NegCont", "Freihändige Vergabe"),
    ("IntNATO", "Internationale Ausschreibung (NATO)"),
    (
        "SelectCallPostOpen",
        "Beschränkte Ausschreibung mit Teilnahmewettbewerb",
    ),
    (
        "NegProcOpen",
        "Verhandlungsverfahren mit Teilnahmewettbewerb",
    ),
    ("CompetDialog", "Wettbewerblicher Dialog"),
    ("InnovationPartnership", "Innovationspartnerschaft"),
];

/// GAEB code → German label; unknown codes stay as written.
pub(crate) fn procedure_label(code: &str) -> String {
    let code = code.trim();
    PROCEDURES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(code))
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| code.to_string())
}

/// German label (or code) → GAEB code; empty when GAEB has none for it.
pub(crate) fn procedure_code(text: &str) -> String {
    let text = text.trim().to_lowercase();
    let text = match text.as_str() {
        "verhandlungsvergabe" => "freihändige vergabe".to_string(),
        _ => text,
    };
    PROCEDURES
        .iter()
        .find(|(code, label)| code.to_lowercase() == text || label.to_lowercase() == text)
        .map(|(code, _)| code.to_string())
        .unwrap_or_default()
}

/// "15.12.2026" (or ISO) → "2026-12-15"; None when it is no date.
pub(crate) fn iso_date(text: &str) -> Option<String> {
    parse_deadline(text).0
}

/// ISO date and time from GAEB back to the free text the user sees.
pub(crate) fn format_deadline(date: Option<&str>, time: Option<&str>) -> String {
    let date = date.map(|iso| {
        let pieces: Vec<&str> = iso.split('-').collect();
        if pieces.len() == 3 {
            format!("{}.{}.{}", pieces[2], pieces[1], pieces[0])
        } else {
            iso.to_string()
        }
    });
    let time = time.map(|time| format!("{} Uhr", time.get(..5).unwrap_or(time)));
    match (date, time) {
        (Some(date), Some(time)) => format!("{date}, {time}"),
        (Some(date), None) => date,
        (None, Some(time)) => time,
        (None, None) => String::new(),
    }
}

/// Up to `limit` entries, then "… und n weitere".
pub(crate) fn short_list(entries: &[String], limit: usize) -> String {
    let mut text = entries
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if entries.len() > limit {
        text.push_str(&format!(" … und {} weitere", entries.len() - limit));
    }
    text
}
