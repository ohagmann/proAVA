use eframe::egui::{self, Color32, Stroke, Visuals};
use proava_application::ThemeChoice;

/// The colours of one appearance. All screens take their colours from here.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub panel: Color32,
    /// Windows and dialogs on top of the panels (slightly lighter in the
    /// dark appearance, so they stand out).
    pub window: Color32,
    pub panel_alt: Color32,
    pub line: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub input: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    pub row_alt: Color32,
    pub row_hover: Color32,
    pub selected: Color32,
    pub on_accent: Color32,
    pub warn: Color32,
    /// Wrong input (field border, message).
    pub error: Color32,
    /// Fill of buttons that delete or discard (white text on it).
    pub danger: Color32,
    /// Title rows: thin line and slightly tinted text.
    pub group_text: Color32,
    /// Row background of a position without quantity (clearly red).
    pub missing_fill: Color32,
    /// Row background of a position without unit or short text (soft yellow).
    pub gap_fill: Color32,
    /// Status „fertig“ (check mark).
    pub done: Color32,
    /// Status „in Bearbeitung“ (pencil).
    pub in_progress: Color32,
    /// Outline of rows with a search hit.
    pub hit: Color32,
    /// KI-Markierung: rows an assistant works on (bridge), and its line in
    /// the status bar – a colour of its own, not used for anything else.
    pub ai: Color32,
    /// Title rows (Titel, Untertitel) of the tables: yellow fill like
    /// „Hellgelb 3/4“ of a spreadsheet (Anwender 18.11.); None = Extras ▸
    /// Einstellungen ▸ „Titelzeilen farbig hinterlegen“ is off.
    pub title_rows: Option<TitleRows>,
    /// Head row of an LV when the table shows several LVs one below the
    /// other: a strong band above the yellow title rows, light text on it.
    pub lv_head: Color32,
    pub lv_head_hover: Color32,
    pub on_lv_head: Color32,
    pub on_lv_head_dim: Color32,
    /// Band above the header while a library (not a project) is edited:
    /// a green of its own, apart from the blue LV heads and the accent.
    pub library_band: Color32,
    pub on_library_band: Color32,
    pub on_library_band_dim: Color32,
}

/// Colours of the title rows: their own fill (light appearance) and the
/// text on them – OZ and title text of the top level and of the levels
/// below.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitleRows {
    /// None: no fill of their own, the rows keep the surface, hover and
    /// selection of every row (dark appearance: only the text is yellow).
    pub fills: Option<TitleFills>,
    /// Further text of the row (sums, „n/m fertig“).
    pub text: Color32,
    pub text_dim: Color32,
    pub title_text: Color32,
    pub subtitle_text: Color32,
}

/// Fill of the top level, of the levels below it (one step lighter) and
/// their hover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TitleFills {
    pub title: Color32,
    pub subtitle: Color32,
    pub title_hover: Color32,
    pub subtitle_hover: Color32,
}

impl Palette {
    pub fn of(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::Light => Self {
                background: rgb(0xF4F5F7),
                panel: rgb(0xFFFFFF),
                window: rgb(0xFFFFFF),
                panel_alt: rgb(0xFAFBFC),
                line: rgb(0xDCE0E5),
                text: rgb(0x1D2125),
                text_dim: rgb(0x5A6371),
                accent: rgb(0x2563C9),
                accent_hover: rgb(0x1F57B3),
                accent_soft: rgb(0xE4EEFB),
                input: rgb(0xFFFFFF),
                button: rgb(0xF1F3F6),
                button_hover: rgb(0xE6EAF0),
                row_alt: rgb(0xF8F9FB),
                row_hover: rgb(0xEBF1FA),
                selected: rgb(0xC7DBFA),
                on_accent: rgb(0xFFFFFF),
                warn: rgb(0x9A5800),
                error: rgb(0xC62828),
                danger: rgb(0xC62828),
                group_text: rgb(0x1A3E78),
                missing_fill: rgb(0xF6C9C9),
                // Light orange: clearly apart from the yellow title rows.
                gap_fill: rgb(0xFFE6BF),
                done: rgb(0x2E7D32),
                in_progress: rgb(0xC26A00),
                // Darker than before: 3:1 also on the yellow title rows.
                hit: rgb(0xB86E00),
                ai: rgb(0x8E24AA),
                // Petrol blue of the print, white text (7:1 and more).
                lv_head: rgb(0x004F7C),
                lv_head_hover: rgb(0x0A5D8F),
                on_lv_head: rgb(0xFFFFFF),
                on_lv_head_dim: rgb(0xD3E4F0),
                library_band: rgb(0x125C45),
                on_library_band: rgb(0xFFFFFF),
                on_library_band_dim: rgb(0xD2EAE0),
                // Hellgelb 3 and Hellgelb 4 of the LibreOffice standard
                // palette; hover slightly darker. The text stays dark.
                title_rows: Some(TitleRows {
                    fills: Some(TitleFills {
                        title: rgb(0xFFFFA6),
                        subtitle: rgb(0xFFFFD7),
                        title_hover: rgb(0xF7F79C),
                        subtitle_hover: rgb(0xF7F7CC),
                    }),
                    text: rgb(0x1D2125),
                    text_dim: rgb(0x5A6371),
                    title_text: rgb(0x1A3E78),
                    subtitle_text: rgb(0x1A3E78),
                }),
            },
            // Neutral dark grey without a blue cast: the surfaces step up from
            // the background (tables' gaps) over the panels to windows and
            // buttons; only the accent, the selection and the meaning colours
            // carry a hue. Contrast (WCAG) checked in the tests below.
            ThemeChoice::Dark => Self {
                background: rgb(0x1B1B1E),
                panel: rgb(0x26262A),
                window: rgb(0x2E2E33),
                panel_alt: rgb(0x2B2B2F),
                line: rgb(0x3C3C43),
                text: rgb(0xECECEF),
                text_dim: rgb(0xAEAEB6),
                accent: rgb(0x5C97F7),
                accent_hover: rgb(0x74A8F8),
                accent_soft: rgb(0x1F2E4A),
                input: rgb(0x1E1E22),
                button: rgb(0x35353B),
                button_hover: rgb(0x3F3F46),
                row_alt: rgb(0x2A2A2E),
                row_hover: rgb(0x34343A),
                selected: rgb(0x233A63),
                on_accent: rgb(0x101012),
                warn: rgb(0xE0A030),
                error: rgb(0xFF7070),
                danger: rgb(0xB3261E),
                group_text: rgb(0xA9C8FF),
                missing_fill: rgb(0x542328),
                // Amber brown: clearly apart from the olive title rows.
                gap_fill: rgb(0x4A3016),
                done: rgb(0x5CC46A),
                in_progress: rgb(0xF0A040),
                hit: rgb(0xF5B342),
                ai: rgb(0xD48CF0),
                // A deep blue band, clearly apart from the yellow title text.
                lv_head: rgb(0x235688),
                lv_head_hover: rgb(0x255A8C),
                on_lv_head: rgb(0xFFFFFF),
                on_lv_head_dim: rgb(0xD0DDEC),
                library_band: rgb(0x245C48),
                on_library_band: rgb(0xFFFFFF),
                on_library_band_dim: rgb(0xCBE0D7),
                // „Gelbe Schrift“ (Anwender 18.11. 22:29): a yellow fill turns
                // olive/ochre on dark grey, so title rows keep the surface of
                // every row (hover, selection as usual) and the yellow goes
                // into OZ and title text, the levels below a little paler.
                title_rows: Some(TitleRows {
                    fills: None,
                    text: rgb(0xECECEF),
                    text_dim: rgb(0xAEAEB6),
                    title_text: rgb(0xF2E58C),
                    subtitle_text: rgb(0xE6DFB0),
                }),
            },
        }
    }
}

impl Palette {
    /// The palette with the title rows switched on or off (Einstellung).
    pub fn with_title_rows(mut self, on: bool) -> Self {
        if !on {
            self.title_rows = None;
        }
        self
    }

    /// Fill of a title row of `level` (0 = top), hovered or not; None when
    /// the title rows are not filled.
    pub fn title_fill(&self, level: usize, hovered: bool) -> Option<Color32> {
        let fills = self.title_rows?.fills?;
        Some(match (level == 0, hovered) {
            (true, false) => fills.title,
            (true, true) => fills.title_hover,
            (false, false) => fills.subtitle,
            (false, true) => fills.subtitle_hover,
        })
    }

    /// The palette for drawing on a filled title row of `level` (0 = top):
    /// its text colours; `group_text` is the colour of OZ and title text.
    pub fn on_title_row(&self, level: usize) -> Palette {
        match self.title_rows {
            Some(rows) => Palette {
                text: rows.text,
                text_dim: rows.text_dim,
                group_text: if level == 0 {
                    rows.title_text
                } else {
                    rows.subtitle_text
                },
                ..*self
            },
            None => *self,
        }
    }
}

/// egui visuals for one appearance.
/// Applies one appearance to every egui theme, so the window never mixes light and dark.
pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_visuals_of(egui::Theme::Dark, visuals(ThemeChoice::Dark));
    ctx.set_visuals_of(egui::Theme::Light, visuals(ThemeChoice::Light));
    ctx.set_theme(match choice {
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}

pub fn visuals(choice: ThemeChoice) -> Visuals {
    let palette = Palette::of(choice);
    let mut visuals = match choice {
        ThemeChoice::Dark => Visuals::dark(),
        ThemeChoice::Light => Visuals::light(),
    };

    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.window;
    visuals.extreme_bg_color = palette.input;
    visuals.faint_bg_color = palette.row_alt;
    visuals.override_text_color = Some(palette.text);
    visuals.hyperlink_color = palette.accent;
    visuals.window_stroke = Stroke::new(1.0, palette.line);
    visuals.selection.bg_fill = palette.selected;
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.dark_mode = choice == ThemeChoice::Dark;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = palette.panel;
    widgets.noninteractive.weak_bg_fill = palette.panel;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, palette.line);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, palette.text);
    widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    widgets.inactive.bg_fill = palette.button;
    widgets.inactive.weak_bg_fill = palette.button;
    widgets.inactive.bg_stroke = Stroke::new(1.0, palette.line);
    widgets.inactive.fg_stroke = Stroke::new(1.0, palette.text);
    widgets.inactive.corner_radius = egui::CornerRadius::same(6);

    widgets.hovered.bg_fill = palette.button_hover;
    widgets.hovered.weak_bg_fill = palette.button_hover;
    widgets.hovered.bg_stroke = Stroke::new(1.0, palette.accent);
    widgets.hovered.fg_stroke = Stroke::new(1.0, palette.text);
    widgets.hovered.corner_radius = egui::CornerRadius::same(6);

    widgets.active.bg_fill = palette.accent;
    widgets.active.weak_bg_fill = palette.accent;
    widgets.active.bg_stroke = Stroke::new(1.0, palette.accent);
    widgets.active.fg_stroke = Stroke::new(1.0, palette.on_accent);
    widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.text_cursor.stroke = Stroke::new(1.6, palette.accent);

    visuals
}

fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        (hex >> 16) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contrast ratio after WCAG 2.x (relative luminance, sRGB).
    fn contrast(a: Color32, b: Color32) -> f32 {
        fn luminance(color: Color32) -> f32 {
            let channel = |value: u8| {
                let value = value as f32 / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
        }
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn at_least(name: &str, text: Color32, on: &[(&str, Color32)], ratio: f32) {
        for (surface, fill) in on {
            let value = contrast(text, *fill);
            assert!(value >= ratio, "{name} auf {surface}: {value:.2} < {ratio}");
        }
    }

    #[test]
    fn the_library_band_stands_out_and_stays_readable() {
        for choice in [ThemeChoice::Light, ThemeChoice::Dark] {
            let p = Palette::of(choice);
            let band = [("library_band", p.library_band)];
            at_least("on_library_band", p.on_library_band, &band, 7.0);
            at_least("on_library_band_dim", p.on_library_band_dim, &band, 4.5);
            let value = contrast(p.library_band, p.panel);
            assert!(
                value >= 1.8,
                "{choice:?}: library_band gegen panel: {value:.2}"
            );
        }
    }

    #[test]
    fn lv_head_rows_stand_out_and_stay_readable() {
        for choice in [ThemeChoice::Light, ThemeChoice::Dark] {
            let p = Palette::of(choice);
            for (name, fill) in [("lv_head", p.lv_head), ("lv_head_hover", p.lv_head_hover)] {
                at_least("on_lv_head", p.on_lv_head, &[(name, fill)], 7.0);
                at_least("on_lv_head_dim", p.on_lv_head_dim, &[(name, fill)], 4.5);
            }
            // Apart from the rows and from the yellow fill of the title rows
            // where they have one (light appearance; the dark one has yellow
            // title text on the row surface).
            let mut others = vec![("panel", p.panel, 1.8), ("row_alt", p.row_alt, 1.8)];
            if let Some(fills) = p.title_rows.expect("Titelzeilen").fills {
                others.push(("title", fills.title, 1.3));
                others.push(("subtitle", fills.subtitle, 1.3));
            }
            for (name, other, least) in others {
                let value = contrast(p.lv_head, other);
                assert!(
                    value >= least,
                    "{choice:?}: lv_head gegen {name}: {value:.2}"
                );
            }
        }
    }

    #[test]
    fn the_dark_appearance_keeps_text_readable() {
        let p = Palette::of(ThemeChoice::Dark);
        let surfaces = [
            ("background", p.background),
            ("panel", p.panel),
            ("window", p.window),
            ("panel_alt", p.panel_alt),
            ("row_alt", p.row_alt),
            ("row_hover", p.row_hover),
            ("input", p.input),
            ("button", p.button),
            ("button_hover", p.button_hover),
            ("selected", p.selected),
            ("accent_soft", p.accent_soft),
            ("missing_fill", p.missing_fill),
            ("gap_fill", p.gap_fill),
        ];
        // Text and the dimmed text everywhere (AA, small text).
        at_least("text", p.text, &surfaces, 7.0);
        at_least("text_dim", p.text_dim, &surfaces, 4.5);
        // Title OZ and the meaning colours on the rows and fields.
        let rows = [
            ("panel", p.panel),
            ("window", p.window),
            ("row_alt", p.row_alt),
            ("row_hover", p.row_hover),
            ("input", p.input),
        ];
        for (name, color) in [
            ("group_text", p.group_text),
            ("warn", p.warn),
            ("error", p.error),
            ("done", p.done),
            ("in_progress", p.in_progress),
            ("ai", p.ai),
        ] {
            at_least(name, color, &rows, 4.5);
        }
        at_least("group_text", p.group_text, &[("selected", p.selected)], 4.5);
        // Accent as text: links, the active tab on its soft fill.
        at_least(
            "accent",
            p.accent,
            &[
                ("panel", p.panel),
                ("window", p.window),
                ("accent_soft", p.accent_soft),
            ],
            4.5,
        );
        // Text on the accent and on the danger button.
        at_least("on_accent", p.on_accent, &[("accent", p.accent)], 4.5);
        at_least("weiß", Color32::WHITE, &[("danger", p.danger)], 4.5);
        // Outlines of search hits (no text): 3:1.
        at_least("hit", p.hit, &rows, 3.0);
    }

    #[test]
    fn the_dark_appearance_is_neutral_grey() {
        let p = Palette::of(ThemeChoice::Dark);
        for (name, color) in [
            ("background", p.background),
            ("panel", p.panel),
            ("window", p.window),
            ("panel_alt", p.panel_alt),
            ("line", p.line),
            ("input", p.input),
            ("button", p.button),
            ("button_hover", p.button_hover),
            ("row_alt", p.row_alt),
            ("row_hover", p.row_hover),
            ("text", p.text),
            ("text_dim", p.text_dim),
        ] {
            let [r, g, b, _] = color.to_array();
            let spread = r.max(g).max(b) - r.min(g).min(b);
            assert!(spread <= 9, "{name} ohne Farbstich: {color:?}");
        }
        // Steps from the background over the panel to windows and buttons.
        let grey = |color: Color32| color.r() as u16 + color.g() as u16 + color.b() as u16;
        assert!(grey(p.background) < grey(p.panel));
        assert!(grey(p.panel) < grey(p.row_alt) && grey(p.row_alt) < grey(p.row_hover));
        assert!(grey(p.panel) < grey(p.window) && grey(p.window) < grey(p.button));
        assert!(grey(p.button) < grey(p.button_hover));
        assert!(
            grey(p.input) < grey(p.panel),
            "Felder dunkler als die Fläche"
        );
        // Windows stand out from the panels; the light appearance stays as it was.
        assert_eq!(visuals(ThemeChoice::Dark).window_fill, p.window);
        let light = Palette::of(ThemeChoice::Light);
        assert_eq!(light.window, light.panel);
        assert_eq!(visuals(ThemeChoice::Light).window_fill, light.panel);
    }

    /// Hue (0–360°) and saturation (HSV) of a colour.
    fn hue_saturation(color: Color32) -> (f32, f32) {
        let [r, g, b, _] = color.to_array().map(|value| value as f32 / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let hue = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let saturation = if max == 0.0 { 0.0 } else { delta / max };
        (hue, saturation)
    }

    /// Hue distance in degrees (0–180).
    fn hue_apart(a: Color32, b: Color32) -> f32 {
        let difference = (hue_saturation(a).0 - hue_saturation(b).0).abs();
        difference.min(360.0 - difference)
    }

    /// Titelzeilen hell (Anwender 18.11. 19:09): gelb hinterlegt wie
    /// „Hellgelb 3/4“, Untertitel eine Stufe heller; Schrift lesbar, die
    /// Markierungen und die Auswahl klar davon getrennt.
    #[test]
    fn light_title_rows_are_yellow_readable_and_apart_from_the_marks() {
        let p = Palette::of(ThemeChoice::Light);
        let rows = p.title_rows.expect("Vorgabe: an");
        let fills = rows.fills.expect("hell mit Fläche");
        assert_eq!(fills.title, rgb(0xFFFFA6), "Hellgelb 3");
        assert_eq!(fills.subtitle, rgb(0xFFFFD7), "Hellgelb 4");
        let surfaces = [
            ("title", fills.title),
            ("subtitle", fills.subtitle),
            ("title_hover", fills.title_hover),
            ("subtitle_hover", fills.subtitle_hover),
        ];
        at_least("Titel-text", rows.text, &surfaces, 7.0);
        at_least("Titel-text_dim", rows.text_dim, &surfaces, 4.5);
        at_least("title_text", rows.title_text, &surfaces, 4.5);
        at_least("subtitle_text", rows.subtitle_text, &surfaces, 4.5);
        // „3/5 fertig“ at the title, outlines of hits and the KI-Markierung.
        at_least("done", p.done, &surfaces, 4.5);
        at_least("hit", p.hit, &surfaces, 3.0);
        at_least("ai", p.ai, &surfaces, 3.0);
        for (name, fill) in surfaces {
            let (hue, saturation) = hue_saturation(fill);
            assert!(
                (50.0..=62.0).contains(&hue) && saturation >= 0.15,
                "{name} ist gelb: {hue:.0}°, {saturation:.2}"
            );
            // „leer“/„GP prüfen“ (gap_fill) and „ohne Menge“ (missing_fill)
            // keep their own hue; the selection is blue.
            assert!(hue_apart(fill, p.gap_fill) >= 18.0, "{name} / gap_fill");
            assert!(
                hue_apart(fill, p.missing_fill) >= 18.0,
                "{name} / missing_fill"
            );
            assert!(hue_apart(fill, p.selected) > 120.0, "{name} / Auswahl");
        }
        let grey = |color: Color32| color.r() as u16 + color.g() as u16 + color.b() as u16;
        assert!(
            grey(fills.subtitle) > grey(fills.title),
            "Untertitel heller"
        );
        assert_eq!(p.title_fill(0, false), Some(fills.title));
        assert_eq!(p.title_fill(2, true), Some(fills.subtitle_hover));
    }

    /// Titelzeilen dunkel (Anwender 18.11. 22:29, „gelbe Schrift“ ohne
    /// Grauband): keine eigene Fläche, OZ und Titeltext hellgelb, Untertitel
    /// blasser – lesbar auf jedem Hintergrund, den eine Titelzeile haben
    /// kann, und klar getrennt von „leer“ (orange).
    #[test]
    fn dark_title_rows_have_yellow_text_on_every_row_background() {
        let p = Palette::of(ThemeChoice::Dark);
        let rows = p.title_rows.expect("Vorgabe: an");
        assert_eq!(rows.fills, None, "keine eigene Fläche");
        assert_eq!(p.title_fill(0, false), None);
        assert_eq!(rows.title_text, rgb(0xF2E58C));
        assert_eq!(rows.subtitle_text, rgb(0xE6DFB0));
        // Fläche, Zebra, Hover, Auswahl, the title fills of Preisspiegel
        // (accent_soft) and Abrechnung (panel_alt), windows, and the
        // KI-Markierung (8 % violet over the row, as in the table).
        let ki = |under: Color32| {
            let mix = |a: u8, b: u8| (a as f32 * 0.92 + b as f32 * 0.08).round() as u8;
            Color32::from_rgb(
                mix(under.r(), p.ai.r()),
                mix(under.g(), p.ai.g()),
                mix(under.b(), p.ai.b()),
            )
        };
        let surfaces = [
            ("panel", p.panel),
            ("background", p.background),
            ("row_alt", p.row_alt),
            ("row_hover", p.row_hover),
            ("selected", p.selected),
            ("accent_soft", p.accent_soft),
            ("panel_alt", p.panel_alt),
            ("window", p.window),
            ("KI auf panel", ki(p.panel)),
            ("KI auf selected", ki(p.selected)),
        ];
        at_least("title_text", rows.title_text, &surfaces, 7.0);
        at_least("subtitle_text", rows.subtitle_text, &surfaces, 7.0);
        // The rest of the row keeps the colours of every row.
        assert_eq!(rows.text, p.text);
        assert_eq!(rows.text_dim, p.text_dim);
        for (name, color) in [
            ("title_text", rows.title_text),
            ("subtitle_text", rows.subtitle_text),
        ] {
            let (hue, saturation) = hue_saturation(color);
            assert!(
                (45.0..=62.0).contains(&hue) && saturation >= 0.2,
                "{name} ist gelb: {hue:.0}°, {saturation:.2}"
            );
            // „leer“: orange fill, amber status text; „in Bearb.“ amber.
            assert!(hue_apart(color, p.gap_fill) >= 18.0, "{name} / gap_fill");
            assert!(
                hue_apart(color, p.in_progress) >= 15.0,
                "{name} / in_progress"
            );
            assert!(hue_apart(color, p.warn) >= 10.0, "{name} / warn");
            let (_, warn_saturation) = hue_saturation(p.warn);
            assert!(
                warn_saturation - saturation >= 0.3,
                "{name} blasser als warn"
            );
        }
        // Untertitel paler than Titel.
        assert!(hue_saturation(rows.subtitle_text).1 < hue_saturation(rows.title_text).1);
        // Switched off: the usual title colour.
        let off = p.with_title_rows(false);
        assert_eq!(off.on_title_row(0).group_text, p.group_text);
        assert_eq!(p.on_title_row(0).group_text, rows.title_text);
        assert_eq!(p.on_title_row(1).group_text, rows.subtitle_text);
    }
}
