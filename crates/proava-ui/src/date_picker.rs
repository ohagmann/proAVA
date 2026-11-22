//! Date fields: type by hand (short input like „1.12.26“ or „1.12.“ is
//! written out when the field is left) or pick from a calendar behind the
//! button with the calendar symbol (Anwender, 17.11.2026: „ein Kalendersymbol,
//! wie die normal aussehen“). The calendar: month and year to switch, weeks
//! from Monday, KW column, „Heute“; arrows, Bild auf/ab, Enter and Esc work.

use eframe::egui::{self, RichText, Stroke};
use proava_domain::GermanDate;

use crate::icons::{self, Icon};
use crate::theme::Palette;

const MONTHS: [&str; 12] = [
    "Januar",
    "Februar",
    "März",
    "April",
    "Mai",
    "Juni",
    "Juli",
    "August",
    "September",
    "Oktober",
    "November",
    "Dezember",
];
const WEEKDAYS: [&str; 7] = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

/// Today, set by the shell once per frame (it owns the clock).
pub fn set_today(ctx: &egui::Context, today_iso: &str) {
    if let Some(today) = GermanDate::parse_iso(today_iso) {
        ctx.data_mut(|data| data.insert_temp(today_id(), today));
    }
}

/// Today as set by the shell; 1.1.2026 when nobody set it (tests).
pub fn today(ctx: &egui::Context) -> GermanDate {
    ctx.data(|data| data.get_temp::<GermanDate>(today_id()))
        .or_else(|| GermanDate::new(1, 1, 2026))
        .expect("valid date")
}

fn today_id() -> egui::Id {
    egui::Id::unique("proava_today")
}

/// What the calendar shows: month on screen and the keyboard day.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CalendarState {
    year: u16,
    month: u8,
    cursor: GermanDate,
}

/// Label, text field with the calendar button and – if any – the error.
/// Leaving the field writes a short date out („1.12.26“ → „01.12.2026“).
pub fn date_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    error: Option<&str>,
) -> egui::Response {
    let caption = ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(3.0);
    let today = today(ui.ctx());
    let mut field_response = None;
    ui.horizontal(|ui| {
        let button_size = 28.0;
        let width = (ui.available_width() - button_size - ui.spacing().item_spacing.x).max(80.0);
        let mut edit = egui::TextEdit::singleline(value)
            .desired_width(width)
            .margin(egui::Margin::symmetric(8, 6))
            .hint_text("TT.MM.JJJJ");
        if error.is_some() {
            edit = edit.text_color(palette.error);
        }
        let response = ui.add(edit).labelled_by(caption.id);
        if response.lost_focus() {
            if let Some(date) = GermanDate::complete(value, today) {
                *value = date.to_string();
            }
        }
        if error.is_some() {
            ui.painter().rect_stroke(
                response.rect,
                egui::CornerRadius::same(4),
                Stroke::new(1.5, palette.error),
                egui::StrokeKind::Outside,
            );
        }
        let button = calendar_button(ui, palette, button_size, &format!("Kalender: {label}"));
        let popup_id = button.id.with("calendar");
        if button.clicked() && !egui::Popup::is_id_open(ui.ctx(), popup_id) {
            // Open on the typed date (or today).
            let shown = GermanDate::complete(value, today).unwrap_or(today);
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    popup_id,
                    CalendarState {
                        year: shown.year(),
                        month: shown.month(),
                        cursor: shown,
                    },
                )
            });
        }
        egui::Popup::from_toggle_button_response(&button)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                let selected = GermanDate::complete(value, today);
                if let Some(picked) = calendar(ui, palette, popup_id, selected, today) {
                    *value = picked.map(|date| date.to_string()).unwrap_or_default();
                    egui::Popup::close_id(ui.ctx(), popup_id);
                }
            });
        field_response = Some(response);
    });
    if let Some(error) = error {
        ui.add_space(2.0);
        ui.label(RichText::new(error).size(11.5).color(palette.error));
    }
    ui.add_space(8.0);
    field_response.expect("drawn above")
}

/// The square button with the calendar symbol (tooltip names it).
fn calendar_button(ui: &mut egui::Ui, palette: &Palette, size: f32, name: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    crate::widgets::name_button(ui, &response, name);
    let background = if response.hovered() {
        palette.button_hover
    } else {
        palette.panel_alt
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(5), background);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(5),
        Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    icons::draw(
        painter,
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(18.0)),
        Icon::Calendar,
        palette.text,
        background,
    );
    response
        .on_hover_text("Kalender")
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The month sheet. Returns `Some(Some(date))` for a picked day,
/// `Some(None)` for „Leeren“, `None` while nothing was picked.
fn calendar(
    ui: &mut egui::Ui,
    palette: &Palette,
    id: egui::Id,
    selected: Option<GermanDate>,
    today: GermanDate,
) -> Option<Option<GermanDate>> {
    let mut state = ui
        .ctx()
        .data(|data| data.get_temp::<CalendarState>(id))
        .unwrap_or(CalendarState {
            year: today.year(),
            month: today.month(),
            cursor: selected.unwrap_or(today),
        });
    let mut picked = None;

    // Keyboard: arrows move the day, Bild auf/ab the month, Enter picks,
    // Esc closes only the calendar (not the dialog behind it).
    let keys = ui.input_mut(|input| {
        let mut take = |key: egui::Key| input.consume_key(egui::Modifiers::NONE, key);
        [
            (take(egui::Key::ArrowLeft), -1i64),
            (take(egui::Key::ArrowRight), 1),
            (take(egui::Key::ArrowUp), -7),
            (take(egui::Key::ArrowDown), 7),
        ]
        .into_iter()
        .filter(|(pressed, _)| *pressed)
        .map(|(_, days)| days)
        .sum::<i64>()
    });
    let (page_up, page_down, enter, escape, home) = ui.input_mut(|input| {
        let none = egui::Modifiers::NONE;
        (
            input.consume_key(none, egui::Key::PageUp),
            input.consume_key(none, egui::Key::PageDown),
            input.consume_key(none, egui::Key::Enter),
            input.consume_key(none, egui::Key::Escape),
            input.consume_key(none, egui::Key::Home),
        )
    });
    if keys != 0 {
        if let Some(cursor) = state.cursor.add_days(keys) {
            state.cursor = cursor;
        }
    }
    if page_up || page_down {
        let (year, month) = shift_month(
            state.cursor.year(),
            state.cursor.month(),
            if page_up { -1 } else { 1 },
        );
        let day = state.cursor.day().min(month_length(year, month));
        if let Some(cursor) = GermanDate::new(day, month, year) {
            state.cursor = cursor;
        }
    }
    if home {
        state.cursor = today;
    }
    if keys != 0 || page_up || page_down || home {
        state.year = state.cursor.year();
        state.month = state.cursor.month();
    }
    if enter {
        picked = Some(Some(state.cursor));
    }
    if escape {
        egui::Popup::close_id(ui.ctx(), id);
    }

    ui.set_min_width(300.0);
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id.with("month"))
            .selected_text(MONTHS[usize::from(state.month - 1)])
            .width(110.0)
            .show_ui(ui, |ui| {
                for (index, name) in MONTHS.iter().enumerate() {
                    ui.selectable_value(&mut state.month, index as u8 + 1, *name);
                }
            });
        let years: Vec<u16> = (today.year().saturating_sub(10)..=today.year() + 10).collect();
        egui::ComboBox::from_id_salt(id.with("year"))
            .selected_text(state.year.to_string())
            .width(70.0)
            .show_ui(ui, |ui| {
                for year in years {
                    ui.selectable_value(&mut state.year, year, year.to_string());
                }
            });
    });
    ui.add_space(4.0);

    let first = GermanDate::new(1, state.month, state.year).unwrap_or(today);
    let start = first.add_days(-i64::from(first.weekday())).unwrap_or(first);
    egui::Grid::new(id.with("grid"))
        .spacing(egui::vec2(2.0, 2.0))
        .min_col_width(32.0)
        .show(ui, |ui| {
            ui.label(RichText::new("KW").size(11.0).color(palette.text_dim));
            for name in WEEKDAYS {
                ui.label(
                    RichText::new(name)
                        .size(11.0)
                        .strong()
                        .color(palette.text_dim),
                );
            }
            ui.end_row();
            for week in 0..6 {
                let Some(monday) = start.add_days(week * 7) else {
                    break;
                };
                if week > 0
                    && monday.month() != state.month
                    && (monday.year(), monday.month()) > (state.year, state.month)
                {
                    break;
                }
                ui.label(
                    RichText::new(monday.iso_week().to_string())
                        .size(11.0)
                        .color(palette.text_dim),
                );
                for offset in 0..7 {
                    let Some(day) = monday.add_days(offset) else {
                        continue;
                    };
                    let in_month = day.month() == state.month;
                    let mut text = RichText::new(day.day().to_string()).size(12.5);
                    if !in_month {
                        text = text.color(palette.text_dim);
                    }
                    let mut button = egui::Button::new(text)
                        .min_size(egui::vec2(32.0, 26.0))
                        .selected(Some(day) == selected);
                    if day == today {
                        button = button.stroke(Stroke::new(1.0, palette.accent));
                    } else if day == state.cursor {
                        button = button.stroke(Stroke::new(1.0, palette.text_dim));
                    } else {
                        button = button.frame(Some(day) == selected);
                    }
                    let response = ui.add(button);
                    let response = if day == state.cursor {
                        response.on_hover_text(day.to_string())
                    } else {
                        response
                    };
                    if response.clicked() {
                        picked = Some(Some(day));
                    }
                }
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button("Heute").clicked() {
            picked = Some(Some(today));
        }
        if ui.button("Leeren").clicked() {
            picked = Some(None);
        }
        ui.label(
            RichText::new(format!("heute {today}"))
                .size(11.0)
                .color(palette.text_dim),
        );
    });
    ui.ctx().data_mut(|data| data.insert_temp(id, state));
    picked
}

fn shift_month(year: u16, month: u8, by: i32) -> (u16, u8) {
    let index = i32::from(year) * 12 + i32::from(month) - 1 + by;
    ((index / 12) as u16, (index % 12) as u8 + 1)
}

fn month_length(year: u16, month: u8) -> u8 {
    GermanDate::new(1, month, year).map_or(28, |date| date.days_in_month())
}

#[cfg(test)]
mod tests {
    use super::{month_length, shift_month};

    #[test]
    fn months_shift_over_the_year() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2027, 1, -1), (2026, 12));
        assert_eq!(shift_month(2026, 11, 0), (2026, 11));
        assert_eq!(month_length(2028, 2), 29);
    }
}
