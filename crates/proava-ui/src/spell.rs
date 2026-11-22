//! Spell checking in the UI: one checker for all text fields, loaded in the
//! background (the dictionary takes a moment), switched by the settings.
//! Fields ask [`speller`] every frame; without dictionary or with the check
//! switched off they get `None` and show nothing.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, Color32, Galley, Painter, Pos2, Stroke};
use proava_application::Launcher;
use proava_spell::{LoadError, Speller};

/// Colour of the wavy line under unknown words.
pub const WAVE_COLOR: Color32 = Color32::from_rgb(0xd2, 0x2b, 0x2b);

#[derive(Clone)]
pub enum SpellStatus {
    Off,
    Loading,
    Ready(Arc<Speller>),
    Missing(String),
}

struct State {
    /// Settings the status belongs to.
    key: Option<(bool, Option<PathBuf>)>,
    status: SpellStatus,
    loading: Option<Receiver<Result<Speller, LoadError>>>,
    /// Words added in a field, to be written to the settings.
    added: Vec<String>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            key: None,
            status: SpellStatus::Off,
            loading: None,
            added: Vec::new(),
        })
    })
}

/// Follows the settings (call once per frame): loads the dictionary when the
/// check is switched on or the folder changes, stores added words.
pub fn configure(launcher: &Launcher) {
    let preferences = launcher.preferences();
    let key = (
        preferences.spell_check,
        preferences.dictionary_folder.clone(),
    );
    let mut state = state().lock().expect("spell state");
    for word in std::mem::take(&mut state.added) {
        launcher.add_personal_word(&word);
    }
    if state.key.as_ref() == Some(&key) {
        return;
    }
    state.key = Some(key.clone());
    state.loading = None;
    if !key.0 {
        state.status = SpellStatus::Off;
        return;
    }
    let (sender, receiver) = channel();
    let personal = launcher.personal_words();
    let folder = key.1;
    std::thread::spawn(move || {
        let _ = sender.send(Speller::find_and_load(folder.as_deref(), &personal));
    });
    state.status = SpellStatus::Loading;
    state.loading = Some(receiver);
}

/// Current status (picks up a finished background load).
pub fn status(ctx: Option<&egui::Context>) -> SpellStatus {
    let mut state = state().lock().expect("spell state");
    if let Some(receiver) = &state.loading {
        match receiver.try_recv() {
            Ok(result) => {
                state.status = match result {
                    Ok(speller) => SpellStatus::Ready(Arc::new(speller)),
                    Err(error) => SpellStatus::Missing(error.to_string()),
                };
                state.loading = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if let Some(ctx) = ctx {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.status =
                    SpellStatus::Missing("Wörterbuch konnte nicht geladen werden.".into());
                state.loading = None;
            }
        }
    }
    state.status.clone()
}

/// The checker, when spell checking is on and a dictionary is loaded.
pub fn speller(ctx: &egui::Context) -> Option<Arc<Speller>> {
    match status(Some(ctx)) {
        SpellStatus::Ready(speller) => Some(speller),
        _ => None,
    }
}

/// Text for the settings dialog.
pub fn status_text() -> String {
    match status(None) {
        SpellStatus::Off => "Rechtschreibprüfung ist ausgeschaltet.".into(),
        SpellStatus::Loading => "Wörterbuch wird geladen …".into(),
        SpellStatus::Missing(message) => message,
        SpellStatus::Ready(speller) => match speller.files() {
            Some(files) => format!(
                "Wörterbuch: {} · {} eigene Wörter",
                files.dic.display(),
                speller.personal_words().len()
            ),
            None => "Wörterbuch geladen.".into(),
        },
    }
}

/// „Zum Wörterbuch hinzufügen“: known at once, stored with the settings.
pub fn add_word(speller: &Speller, word: &str) {
    speller.add_personal(word);
    state()
        .lock()
        .expect("spell state")
        .added
        .push(word.to_string());
}

/// Screen spans (x from, x to, row bottom) of chars `from..to` of a galley
/// painted at `origin`.
pub fn char_spans(galley: &Galley, origin: Pos2, from: usize, to: usize) -> Vec<(f32, f32, f32)> {
    let mut spans = Vec::new();
    let mut index = 0usize;
    for row in &galley.rows {
        let count = row.char_count_including_newline().0;
        let visible = row.char_count_excluding_newline().0;
        let a = from.max(index);
        let b = to.min(index + visible);
        if a < b {
            let x = |at: usize| {
                row.glyphs
                    .get(at - index)
                    .map_or(row.rect().max.x, |glyph| row.pos.x + glyph.pos.x)
            };
            let x0 = x(a);
            let x1 = if b == index + visible {
                row.rect().max.x
            } else {
                x(b)
            };
            spans.push((origin.x + x0, origin.x + x1, origin.y + row.rect().max.y));
        }
        index += count;
    }
    spans
}

/// Red wavy line from `x0` to `x1` just above `y`.
pub fn wave(painter: &Painter, x0: f32, x1: f32, y: f32) {
    if x1 - x0 < 2.0 {
        return;
    }
    let amplitude = 1.5;
    let step = 2.5;
    let y = y - 1.5;
    let mut points = Vec::new();
    let mut x = x0;
    let mut up = true;
    while x <= x1 {
        points.push(Pos2::new(x, if up { y - amplitude } else { y + amplitude }));
        x += step;
        up = !up;
    }
    painter.add(egui::Shape::line(points, Stroke::new(1.0, WAVE_COLOR)));
}

/// Wavy lines under the unknown words of `text` laid out in `galley` at
/// `origin`; `skip` = char position of a caret (the word typed right now
/// is not marked yet). Returns the unknown words (char ranges).
pub fn mark(
    painter: &Painter,
    speller: &Speller,
    text: &str,
    galley: &Galley,
    origin: Pos2,
    skip: Option<usize>,
) -> Vec<(Range<usize>, String)> {
    let mut unknown = Vec::new();
    for word in speller.unknown_words(text) {
        if skip.is_some_and(|caret| word.chars.start <= caret && caret <= word.chars.end) {
            continue;
        }
        for (x0, x1, y) in char_spans(galley, origin, word.chars.start, word.chars.end) {
            wave(painter, x0, x1, y);
        }
        unknown.push((word.chars.clone(), word.text));
    }
    unknown
}

/// For plain text fields (Kurztext, table cell): wavy lines under unknown
/// words and, on right click on such a word, suggestions, „Zum Wörterbuch
/// hinzufügen“ and „Ignorieren“ in the context menu. Returns true when a
/// suggestion replaced the word in `text`.
pub fn text_edit(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text: &mut String,
) -> bool {
    text_edit_with_menu(ui, output, text, None::<fn(&mut egui::Ui)>)
}

/// Like [`text_edit`]; `extra` fills the context menu below the suggestions
/// (the table cell puts the row menu there). Without a marked word under the
/// right click the menu shows only `extra` – or does not open at all.
pub fn text_edit_with_menu(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text: &mut String,
    extra: Option<impl FnOnce(&mut egui::Ui)>,
) -> bool {
    let Some(speller) = speller(ui.ctx()) else {
        if let Some(extra) = extra {
            output.response.context_menu(extra);
        }
        return false;
    };
    let caret = output
        .response
        .has_focus()
        .then(|| output.cursor_range.map(|range| range.primary.index.0))
        .flatten();
    let painter = ui.painter_at(output.response.rect);
    let unknown = mark(
        &painter,
        &speller,
        text,
        &output.galley,
        output.galley_pos,
        caret,
    );
    let menu_id = output.response.id.with("spell_word");
    // Word under a right click, remembered for the menu.
    let clicked = ui.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed: true,
                ..
            } if output.response.rect.contains(*pos) => Some(*pos),
            _ => None,
        })
    });
    if let Some(pos) = clicked {
        let at = output
            .galley
            .cursor_from_pos(pos - output.galley_pos)
            .index
            .0;
        let word = unknown
            .iter()
            .find(|(range, _)| range.start <= at && at <= range.end)
            .cloned();
        ui.ctx().data_mut(|data| {
            data.insert_temp::<Option<(Range<usize>, String, Option<Vec<String>>)>>(
                menu_id,
                word.map(|(range, word)| (range, word, None)),
            )
        });
    }
    let mut changed = false;
    let word_under_click = ui
        .ctx()
        .data(|data| data.get_temp::<Option<(Range<usize>, String, Option<Vec<String>>)>>(menu_id));
    let Some(Some((range, word, suggestions))) = word_under_click else {
        // No marked word: the row menu (or nothing), never an empty menu.
        if let Some(extra) = extra {
            output.response.context_menu(extra);
        }
        return false;
    };
    output.response.context_menu(|ui| {
        let suggestions = suggestions.unwrap_or_else(|| {
            let list = speller.suggest(&word, 6);
            ui.ctx().data_mut(|data| {
                data.insert_temp::<Option<(Range<usize>, String, Option<Vec<String>>)>>(
                    menu_id,
                    Some((range.clone(), word.clone(), Some(list.clone()))),
                )
            });
            list
        });
        if let Some(choice) = suggestion_items(ui, &speller, &word, &suggestions) {
            let chars: Vec<char> = text.chars().collect();
            if range.end <= chars.len() {
                let before: String = chars[..range.start].iter().collect();
                let after: String = chars[range.end..].iter().collect();
                *text = format!("{before}{choice}{after}");
                changed = true;
            }
        }
        if let Some(extra) = extra {
            ui.separator();
            extra(ui);
        }
    });
    changed
}

/// Menu entries for an unknown word: suggestions (returned when chosen),
/// „Zum Wörterbuch hinzufügen“, „Ignorieren“.
pub fn suggestion_items(
    ui: &mut egui::Ui,
    speller: &Speller,
    word: &str,
    suggestions: &[String],
) -> Option<String> {
    let mut chosen = None;
    if suggestions.is_empty() {
        ui.label(egui::RichText::new("Keine Vorschläge").italics());
    }
    for suggestion in suggestions {
        if ui
            .button(
                egui::RichText::new(suggestion)
                    .font(crate::fonts::bold(14.0))
                    .color(ui.visuals().text_color()),
            )
            .clicked()
        {
            chosen = Some(suggestion.clone());
            ui.close();
        }
    }
    ui.separator();
    if ui
        .button(format!("„{word}“ zum Wörterbuch hinzufügen"))
        .clicked()
    {
        add_word(speller, word);
        ui.close();
    }
    if ui.button("Ignorieren").clicked() {
        speller.ignore(word);
        ui.close();
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_follow_rows() {
        let ctx = egui::Context::default();
        let mut spans = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx();
            let galley = ctx.fonts_mut(|fonts| {
                fonts.layout(
                    "Wort eins\nzwei".to_string(),
                    egui::FontId::proportional(14.0),
                    Color32::BLACK,
                    f32::INFINITY,
                )
            });
            spans = char_spans(&galley, Pos2::new(10.0, 20.0), 5, 14);
        });
        output.textures_delta.clear();
        assert_eq!(spans.len(), 2, "{spans:?}");
        assert!(spans[0].0 > 10.0 && spans[0].1 > spans[0].0);
        assert!(spans[1].2 > spans[0].2, "zweite Zeile tiefer");
    }
}
