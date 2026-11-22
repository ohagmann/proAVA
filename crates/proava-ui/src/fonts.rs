//! Fonts: Liberation Sans (SIL OFL, see `fonts/OFL.txt`) with real bold and
//! italic faces, egui's built-in fonts stay as fallback. Liberation Serif and
//! Mono (Times/Courier-compatible) serve the rich text editor.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use proava_domain::FontClass;

const REGULAR: &[u8] = include_bytes!("../fonts/LiberationSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/LiberationSans-Bold.ttf");
const ITALIC: &[u8] = include_bytes!("../fonts/LiberationSans-Italic.ttf");
const BOLD_ITALIC: &[u8] = include_bytes!("../fonts/LiberationSans-BoldItalic.ttf");

const FACES: [(&str, &[u8]); 8] = [
    (
        "Serif",
        include_bytes!("../fonts/LiberationSerif-Regular.ttf"),
    ),
    (
        "Serif-Bold",
        include_bytes!("../fonts/LiberationSerif-Bold.ttf"),
    ),
    (
        "Serif-Italic",
        include_bytes!("../fonts/LiberationSerif-Italic.ttf"),
    ),
    (
        "Serif-BoldItalic",
        include_bytes!("../fonts/LiberationSerif-BoldItalic.ttf"),
    ),
    (
        "Mono",
        include_bytes!("../fonts/LiberationMono-Regular.ttf"),
    ),
    (
        "Mono-Bold",
        include_bytes!("../fonts/LiberationMono-Bold.ttf"),
    ),
    (
        "Mono-Italic",
        include_bytes!("../fonts/LiberationMono-Italic.ttf"),
    ),
    (
        "Mono-BoldItalic",
        include_bytes!("../fonts/LiberationMono-BoldItalic.ttf"),
    ),
];

pub const FAMILY_BOLD: &str = "bold";
pub const FAMILY_ITALIC: &str = "italic";
pub const FAMILY_BOLD_ITALIC: &str = "bold-italic";

/// Installs Liberation Sans as the proportional font and adds the bold,
/// italic and bold-italic families.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("Liberation", REGULAR),
        ("Liberation-Bold", BOLD),
        ("Liberation-Italic", ITALIC),
        ("Liberation-BoldItalic", BOLD_ITALIC),
    ] {
        fonts
            .font_data
            .insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
    }
    let fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let with_fallback = |first: &str| {
        let mut list = vec![first.to_string()];
        list.extend(fallback.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, with_fallback("Liberation"));
    for (family, face) in [
        (FAMILY_BOLD, "Liberation-Bold"),
        (FAMILY_ITALIC, "Liberation-Italic"),
        (FAMILY_BOLD_ITALIC, "Liberation-BoldItalic"),
    ] {
        fonts
            .families
            .insert(FontFamily::Name(family.into()), with_fallback(face));
    }
    for (name, bytes) in FACES {
        let face = format!("Liberation-{name}");
        fonts
            .font_data
            .insert(face.clone(), Arc::new(FontData::from_static(bytes)));
        fonts.families.insert(
            FontFamily::Name(name.to_lowercase().into()),
            with_fallback(&face),
        );
    }
    fonts
}

/// Face of the rich text editor: family class, bold, italic.
pub fn rich(size: f32, class: FontClass, bold: bool, italic: bool) -> FontId {
    let base = match class {
        FontClass::Sans => return styled(size, bold, italic),
        FontClass::Serif => "serif",
        FontClass::Mono => "mono",
    };
    let style = match (bold, italic) {
        (true, true) => "-bolditalic",
        (true, false) => "-bold",
        (false, true) => "-italic",
        (false, false) => "",
    };
    FontId::new(size, FontFamily::Name(format!("{base}{style}").into()))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_BOLD.into()))
}

pub fn italic(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY_ITALIC.into()))
}

/// Face for a combination of bold and italic.
pub fn styled(size: f32, bold: bool, italic: bool) -> FontId {
    let family = match (bold, italic) {
        (true, true) => FontFamily::Name(FAMILY_BOLD_ITALIC.into()),
        (true, false) => FontFamily::Name(FAMILY_BOLD.into()),
        (false, true) => FontFamily::Name(FAMILY_ITALIC.into()),
        (false, false) => FontFamily::Proportional,
    };
    FontId::new(size, family)
}

#[cfg(test)]
mod tests {
    use eframe::egui::FontFamily;

    use super::{definitions, FAMILY_BOLD};

    #[test]
    fn families_have_liberation_first_and_fallbacks() {
        let fonts = definitions();
        let proportional = &fonts.families[&FontFamily::Proportional];
        assert_eq!(proportional[0], "Liberation");
        assert!(proportional.len() > 1);
        let bold = &fonts.families[&FontFamily::Name(FAMILY_BOLD.into())];
        assert_eq!(bold[0], "Liberation-Bold");
        for face in [
            "Liberation",
            "Liberation-Bold",
            "Liberation-Italic",
            "Liberation-BoldItalic",
        ] {
            assert!(fonts.font_data.contains_key(face));
        }
        for family in ["serif", "serif-bold", "mono-italic", "mono-bolditalic"] {
            assert!(
                fonts
                    .families
                    .contains_key(&FontFamily::Name(family.into())),
                "{family}"
            );
        }
    }
}
