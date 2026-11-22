//! Kopf-/Fußzeile in `projektangaben_setzen` (eigene des Projekts) and
//! `pdf_drucken` (nur für diesen Ausdruck).

use proava_domain::{FrameLogoPlace, PageFrame, FRAME_FONT_SIZES};
use serde_json::{json, Value};

use crate::args::Args;

/// Keys of the frame fields: (Schlüssel, Kopf?, Feld 0–2).
const FIELDS: [(&str, bool, usize); 6] = [
    ("kopf_links", true, 0),
    ("kopf_mitte", true, 1),
    ("kopf_rechts", true, 2),
    ("fuss_links", false, 0),
    ("fuss_mitte", false, 1),
    ("fuss_rechts", false, 2),
];

/// Applies the frame keys of `args` to `frame`; true if one was given.
pub(crate) fn apply(frame: &mut PageFrame, args: &Args) -> Result<bool, String> {
    let mut changed = false;
    for (key, head, index) in FIELDS {
        if let Some(text) = args.opt_text(key)? {
            let field = if head {
                &mut frame.header[index]
            } else {
                &mut frame.footer[index]
            };
            *field = text.to_string();
            changed = true;
        }
    }
    if let Some(on) = args.opt_flag("kopf_linie")? {
        frame.header_rule = on;
        changed = true;
    }
    if let Some(on) = args.opt_flag("fuss_linie")? {
        frame.footer_rule = on;
        changed = true;
    }
    if let Some(place) = args.opt_text("kopf_logo")? {
        frame.logo = FrameLogoPlace::from_key(place)
            .ok_or_else(|| format!("kopf_logo: „{place}“ – bitte aus, links oder rechts."))?;
        changed = true;
    }
    if let Some(size) = args.opt_count("kopf_schrift_pt")? {
        if !FRAME_FONT_SIZES.contains(&(size as u8)) || size > 255 {
            return Err("kopf_schrift_pt: bitte 7 bis 10.".to_string());
        }
        frame.font_size_pt = size as u8;
        changed = true;
    }
    if changed {
        *frame = frame.normalized();
        let unknown = frame.unknown_placeholders();
        if !unknown.is_empty() {
            return Err(format!(
                "Unbekannte Platzhalter: {} – bekannt sind {}.",
                unknown
                    .iter()
                    .map(|name| format!("{{{name}}}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                placeholder_list()
            ));
        }
    }
    Ok(changed)
}

pub(crate) fn placeholder_list() -> String {
    proava_domain::PAGE_PLACEHOLDERS
        .iter()
        .map(|(name, _)| format!("{{{name}}}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Catalog entries of the frame keys; their rules and the placeholders are
/// in the server instructions.
pub(crate) fn props() -> Vec<(&'static str, Value)> {
    let field = || json!({"type": "string"});
    vec![
        ("kopf_links", field()),
        ("kopf_mitte", field()),
        ("kopf_rechts", field()),
        ("fuss_links", field()),
        ("fuss_mitte", field()),
        ("fuss_rechts", field()),
        ("kopf_linie", json!({"type": "boolean"})),
        ("fuss_linie", json!({"type": "boolean"})),
        ("kopf_logo", json!({"type": "string"})),
        ("kopf_schrift_pt", json!({"type": "integer"})),
    ]
}

/// The frame as text for answers.
pub(crate) fn describe(frame: &PageFrame) -> String {
    let side = |fields: &[String; 3]| {
        fields
            .iter()
            .map(|field| {
                if field.is_empty() {
                    "–".to_string()
                } else {
                    field.replace('\n', " / ")
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    format!(
        "Kopf: {} (Linie {}), Fuß: {} (Linie {}), Logo {}, {} pt",
        side(&frame.header),
        if frame.header_rule { "an" } else { "aus" },
        side(&frame.footer),
        if frame.footer_rule { "an" } else { "aus" },
        frame.logo.key(),
        frame.font_size_pt
    )
}
