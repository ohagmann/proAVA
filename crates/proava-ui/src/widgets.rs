//! Small shared pieces of every dialog: the dimmed scrim, the window and its buttons.

use eframe::egui::{self, RichText, Stroke};

use crate::theme::Palette;

/// A modal dialog: the window sits centred over a dark backdrop that blocks the rest.
/// Returns the content's result and whether the dialog should close (backdrop click, Esc).
pub fn modal<R>(
    ui: &mut egui::Ui,
    palette: &Palette,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
    let id = ui.make_persistent_id("dialog_body");
    modal_with_id(ui, palette, id, width, add_contents)
}

/// Like [`modal`] with its own id – for a question on top of another
/// dialog (shown after it, it is the top one and gets Esc and the keys).
pub fn modal_with_id<R>(
    ui: &mut egui::Ui,
    palette: &Palette,
    id: egui::Id,
    width: f32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, bool) {
    let ctx = ui.ctx().clone();
    let frame = egui::Frame::new()
        .fill(palette.window)
        .stroke(Stroke::new(1.0, palette.line))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(18));

    let modal = egui::Modal::new(id)
        .backdrop_color(egui::Color32::from_black_alpha(170))
        .frame(frame)
        .area(
            egui::Modal::default_area(id)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::new(0.0, 20.0)),
        );

    let response = modal.show(&ctx, |ui| {
        ui.set_width(width);
        add_contents(ui)
    });
    keep_popups_above(&ctx, response.response.layer_id, response.is_top_modal);

    let close = response.should_close();
    (response.inner, close)
}

/// Esc for a floating window over the LV (egui::Window such as „Kosten nach
/// Kostengruppen“): closes it right away when nothing else wants the key –
/// no field with the caret, no open list or menu, no dialog on top. Takes
/// the key then.
pub fn esc_closes_window(ctx: &egui::Context) -> bool {
    if ctx.text_edit_focused()
        || egui::Popup::is_any_open(ctx)
        || ctx.memory(|memory| memory.top_modal_layer().is_some())
    {
        return false;
    }
    ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

/// Keeps the open lists and menus of the top dialog above it.
///
/// egui 0.37 lifts a popup (Auswahlliste, Menü, Kalender) above the dialog
/// only by putting its layer on top when it appears. A dialog shown a second
/// time is lifted the same way and lands above the popup layer egui already
/// knows from the first time; a quick click (press and release in one frame)
/// lifts both, and egui keeps the old order – the list opened, but stayed
/// behind the dialog (18.11.2026: „Neues Projekt“ after „Projekte“, the
/// Vorlage could no longer be chosen). So every visible layer of the same
/// order that is no dialog becomes a sublayer of the top dialog: egui draws it
/// right above the dialog and lets it take the clicks.
fn keep_popups_above(ctx: &egui::Context, dialog: egui::LayerId, is_top: bool) {
    let known = egui::Id::unique("proava_dialog_layers");
    let dialogs = ctx.data_mut(|data| {
        let layers = data.get_temp_mut_or_default::<Vec<egui::LayerId>>(known);
        if !layers.contains(&dialog) {
            layers.push(dialog);
        }
        layers.clone()
    });
    if !is_top {
        return;
    }
    let popups: Vec<egui::LayerId> = ctx.memory(|memory| {
        memory
            .layer_ids()
            .filter(|layer| {
                layer.order == dialog.order
                    && !dialogs.contains(layer)
                    && memory.areas().is_visible(layer)
            })
            .collect()
    });
    for popup in popups {
        ctx.set_sublayer(dialog, popup);
    }
}

pub fn title(ui: &mut egui::Ui, palette: &Palette, title_text: &str, hint: &str) {
    ui.label(
        RichText::new(title_text)
            .size(16.0)
            .strong()
            .color(palette.text),
    );
    ui.add_space(3.0);
    if !hint.is_empty() {
        ui.label(RichText::new(hint).size(12.0).color(palette.text_dim));
    }
    ui.add_space(14.0);
}

pub fn form_field(ui: &mut egui::Ui, palette: &Palette, label: &str, value: &mut String) {
    ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(4.0);
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(ui.available_width())
            .margin(egui::Margin::symmetric(8, 7)),
    );
    ui.add_space(10.0);
}

pub fn error_line(ui: &mut egui::Ui, palette: &Palette, error: Option<&str>) {
    if let Some(text) = error {
        ui.label(RichText::new(text).size(12.5).color(palette.error));
        ui.add_space(8.0);
    }
}

/// Cancel left, confirm right. Returns `(cancelled, confirmed)`.
pub fn dialog_buttons(ui: &mut egui::Ui, palette: &Palette, confirm_label: &str) -> (bool, bool) {
    dialog_buttons_styled(ui, palette, confirm_label, false)
}

/// Like [`dialog_buttons`], but the confirm button is red: for questions that
/// delete or discard something („Löschen“, „Entfernen“, „Verwerfen“).
pub fn danger_dialog_buttons(
    ui: &mut egui::Ui,
    palette: &Palette,
    confirm_label: &str,
) -> (bool, bool) {
    dialog_buttons_styled(ui, palette, confirm_label, true)
}

/// A red button with white text for deleting or discarding.
pub fn danger_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    width: f32,
) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(13.0).color(egui::Color32::WHITE))
            .fill(palette.danger)
            .min_size(egui::vec2(width, 30.0)),
    )
}

fn dialog_buttons_styled(
    ui: &mut egui::Ui,
    palette: &Palette,
    confirm_label: &str,
    danger: bool,
) -> (bool, bool) {
    let mut cancelled = false;
    let mut confirmed = false;
    let (fill, text) = if danger {
        (palette.danger, egui::Color32::WHITE)
    } else {
        (palette.accent, palette.on_accent)
    };

    button_row(ui, |ui| {
        if ui
            .add(
                egui::Button::new(RichText::new(confirm_label).color(text))
                    .fill(fill)
                    .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            confirmed = true;
        }
        ui.add_space(8.0);
        if ui
            .add(
                egui::Button::new(RichText::new(crate::strings::CANCEL).color(palette.text))
                    .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            cancelled = true;
        }
    });

    (cancelled, confirmed)
}

/// Like [`dialog_buttons`] with a third, plain button left of „Abbrechen“
/// (another way out of the question). Returns (cancelled, confirmed, extra).
pub fn dialog_buttons_with_extra(
    ui: &mut egui::Ui,
    palette: &Palette,
    confirm_label: &str,
    extra_label: &str,
) -> (bool, bool, bool) {
    let (mut cancelled, mut confirmed, mut extra) = (false, false, false);
    button_row(ui, |ui| {
        let size = egui::vec2(120.0, 32.0);
        confirmed = ui
            .add(
                egui::Button::new(RichText::new(confirm_label).color(palette.on_accent))
                    .fill(palette.accent)
                    .min_size(size),
            )
            .clicked();
        ui.add_space(8.0);
        cancelled = ui
            .add(
                egui::Button::new(RichText::new(crate::strings::CANCEL).color(palette.text))
                    .min_size(size),
            )
            .clicked();
        ui.add_space(8.0);
        extra = ui
            .add(egui::Button::new(RichText::new(extra_label).color(palette.text)).min_size(size))
            .clicked();
    });
    (cancelled, confirmed, extra)
}

/// One accent button, right aligned (for dialogs with a single "Fertig").
pub fn accent_button(ui: &mut egui::Ui, palette: &Palette, label: &str) -> bool {
    let mut clicked = false;
    button_row(ui, |ui| {
        if ui
            .add(
                egui::Button::new(RichText::new(label).color(palette.on_accent))
                    .fill(palette.accent)
                    .min_size(egui::vec2(120.0, 32.0)),
            )
            .clicked()
        {
            clicked = true;
        }
    });
    clicked
}

/// Like [`form_field`], but takes the keyboard focus once when `focus` is set.
pub fn focus_field(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    value: &mut String,
    focus: &mut bool,
) -> egui::Response {
    ui.label(RichText::new(label).size(12.0).color(palette.text_dim));
    ui.add_space(4.0);
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(ui.available_width())
            .margin(egui::Margin::symmetric(8, 7)),
    );
    if std::mem::take(focus) {
        response.request_focus();
    }
    ui.add_space(10.0);
    response
}

/// Value to store for a text field that applies every change at once
/// (a typed space must show at once, not only with the next character): while typing the text as it is, trimmed only when the field is
/// left. None = nothing to store (no change, or the same as `stored`).
pub fn live_text(response: &egui::Response, text: &str, stored: &str) -> Option<String> {
    if !(response.changed() || response.lost_focus()) {
        return None;
    }
    let value = if response.lost_focus() {
        text.trim()
    } else {
        text
    };
    (value != stored).then(|| value.to_string())
}

/// Gives a self-drawn button its name for screen readers and UI tests
/// (AccessKit); egui's own buttons do that themselves.
pub fn name_button(ui: &egui::Ui, response: &egui::Response, label: &str) {
    let enabled = ui.is_enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
}

/// Gives a self-drawn list row its name and selection state for screen
/// readers and UI tests (AccessKit), like the rows of the LV table.
pub fn name_row(response: &egui::Response, label: &str, selected: bool) {
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Row, true, selected, label));
}

/// Paints the selection of a list row like the LV table: the row in
/// `palette.selected` with a bar in the accent colour on the left.
pub fn selection_bar(painter: &egui::Painter, palette: &Palette, rect: egui::Rect) {
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
        egui::CornerRadius::ZERO,
        palette.accent,
    );
}

/// Toolbar button that shows only a symbol, like the paragraph buttons in a
/// word processor (egui frame, `selected` state). `hint` is the tooltip and
/// the name for screen readers and UI tests.
pub fn tool_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: crate::icons::Icon,
    selected: bool,
    hint: &str,
) -> egui::Response {
    let response = ui.add(
        egui::Button::new("")
            .selected(selected)
            .min_size(egui::Vec2::new(28.0, 26.0)),
    );
    name_button(ui, &response, hint);
    let color = if ui.is_enabled() {
        palette.text
    } else {
        palette.text_dim
    };
    let backdrop = if selected {
        ui.visuals().selection.bg_fill
    } else if response.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        ui.visuals().widgets.inactive.weak_bg_fill
    };
    crate::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(response.rect.center(), egui::Vec2::splat(16.0)),
        icon,
        color,
        backdrop,
    );
    response.on_hover_text(hint)
}

/// Small framed button with a symbol and a label (e.g. back arrow with
/// "Projekte" in the header, magnifier with "Befehl …").
pub fn icon_label_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: crate::icons::Icon,
    label: &str,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(12.5),
        palette.text,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 40.0, 28.0),
        egui::Sense::click(),
    );
    name_button(ui, &response, label);
    // Greyed out in a disabled ui (no selection yet).
    let enabled = ui.is_enabled();
    let background = if enabled && response.hovered() {
        palette.button_hover
    } else {
        palette.button
    };
    let color = if enabled {
        palette.text
    } else {
        palette.text_dim.gamma_multiply(0.7)
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(6), background);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    crate::icons::draw(
        painter,
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + 15.0, rect.center().y),
            egui::Vec2::splat(13.0),
        ),
        icon,
        color,
        background,
    );
    painter.galley(
        egui::pos2(rect.left() + 27.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Button of the tool row above a list (Bearbeiten, Entfernen …): greyed
/// out without a selection, with `disabled_hint` as tooltip then
/// (Anwender 18.11.: „Erst einen Bieter wählen“).
pub fn list_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: Option<crate::icons::Icon>,
    label: &str,
    enabled: bool,
    disabled_hint: &str,
) -> egui::Response {
    let response = ui
        .add_enabled_ui(enabled, |ui| match icon {
            Some(icon) => icon_label_button(ui, palette, icon, label),
            None => ui.add(
                egui::Button::new(RichText::new(label).size(12.5)).min_size(egui::vec2(0.0, 28.0)),
            ),
        })
        .inner;
    response.on_disabled_hover_text(disabled_hint)
}

/// Magnifier in front of a search field (marks the field, not clickable).
pub fn search_mark(ui: &mut egui::Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(18.0), egui::Sense::hover());
    crate::icons::draw(
        ui.painter(),
        rect.shrink(1.0),
        crate::icons::Icon::Search,
        palette.text_dim,
        palette.background,
    );
}

/// Small framed text button (text only, for actions without a common symbol).
pub fn small_button(ui: &mut egui::Ui, palette: &Palette, label: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(12.5),
        palette.text,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 22.0, 28.0),
        egui::Sense::click(),
    );
    name_button(ui, &response, label);
    let background = if response.hovered() {
        palette.button_hover
    } else {
        palette.button
    };
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(6), background);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        Stroke::new(1.0, palette.line),
        egui::StrokeKind::Inside,
    );
    painter.galley(
        egui::pos2(rect.left() + 11.0, rect.center().y - galley.size().y / 2.0),
        galley,
        palette.text,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Square header button with a symbol (gear for Einstellungen, sun/moon for
/// the look) and a tooltip that names it.
pub fn square_icon_button(
    ui: &mut egui::Ui,
    palette: &Palette,
    icon: crate::icons::Icon,
    hint: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::click());
    name_button(ui, &response, hint);
    let background = if response.hovered() {
        palette.button_hover
    } else {
        palette.panel_alt
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(7), background);
    crate::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(16.0)),
        icon,
        palette.text,
        background,
    );
    response
        .on_hover_text(hint)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The text of a table cell on one line (Anwender 18.11.): line breaks become
/// spaces ([`crate::format::one_line`]), a text wider than `width` in `font`
/// ends with „…“ (measured, not estimated). Returns the shown text and whether
/// it was shortened – then the full text belongs in the tooltip.
pub fn fit_line(
    painter: &egui::Painter,
    text: &str,
    font: &egui::FontId,
    width: f32,
) -> (String, bool) {
    let text = crate::format::one_line(text);
    let measure = |piece: String| {
        painter
            .layout_no_wrap(piece, font.clone(), egui::Color32::WHITE)
            .size()
            .x
    };
    if measure(text.to_string()) <= width {
        return (text.into_owned(), false);
    }
    let characters: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0, characters.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate: String = characters[..middle].iter().collect();
        if measure(format!("{candidate}…")) <= width {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let kept: String = characters[..low].iter().collect();
    (format!("{}…", kept.trim_end()), true)
}

/// Paints the text of a table cell on one line, shortened with „…“ to `width`
/// ([`fit_line`]). Returns the painted rect and whether the text was cut.
pub fn cell_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    align: egui::Align2,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> (egui::Rect, bool) {
    let (shown, cut) = fit_line(painter, text, &font, width);
    (painter.text(pos, align, shown, font, color), cut)
}

/// Text in the bold face of the same size.
pub fn bold_text(
    painter: &egui::Painter,
    pos: egui::Pos2,
    align: egui::Align2,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
) -> egui::Rect {
    painter.text(pos, align, text, crate::fonts::bold(font.size), color)
}

/// Small rounded label like "B+GB" or "Psch".
pub fn badge(
    painter: &egui::Painter,
    left_center: egui::Pos2,
    text: &str,
    fill: egui::Color32,
    color: egui::Color32,
) -> f32 {
    let font = egui::FontId::proportional(10.5);
    let galley = painter.layout_no_wrap(text.to_string(), font, color);
    let size = galley.size() + egui::vec2(8.0, 2.0);
    let rect = egui::Rect::from_min_size(
        egui::pos2(left_center.x, left_center.y - size.y / 2.0),
        size,
    );
    painter.rect_filled(rect, egui::CornerRadius::same(4), fill);
    painter.galley(rect.min + egui::vec2(4.0, 1.0), galley, color);
    size.x
}

/// The mark of a linked Ausführungsbeschreibung in a table row („A0001“):
/// a bit larger than [`badge`], light ground with a thin accent frame, so it
/// stays readable on a selected (light blue) row too. Returns its width.
pub fn link_badge(
    painter: &egui::Painter,
    left_center: egui::Pos2,
    text: &str,
    palette: &Palette,
) -> f32 {
    let font = egui::FontId::proportional(11.5);
    let galley = painter.layout_no_wrap(text.to_string(), font, palette.accent);
    let size = galley.size() + egui::vec2(10.0, 2.0);
    let rect = egui::Rect::from_min_size(
        egui::pos2(left_center.x, left_center.y - size.y / 2.0),
        size,
    );
    painter.rect(
        rect,
        egui::CornerRadius::same(4),
        palette.panel,
        egui::Stroke::new(1.0, palette.accent.gamma_multiply(0.7)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + egui::vec2(5.0, 1.0), galley, palette.accent);
    size.x
}

/// A row of labels, fields and buttons whose vertical middles lie on one
/// line (Anwender 18.11.: Text und Knopf mittig zueinander). egui centres an
/// item only within the row height known when the item is added, so a label
/// in front of a taller button or field sat higher. This row starts with the
/// height of its tallest item (remembered from the last pass; when an item is
/// taller, the pass runs once more, invisibly).
pub fn row<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    row_with(ui, false, Box::new(add_contents))
}

/// Like [`row`], continued on further lines when the space is too narrow;
/// every line takes the height of the tallest item.
pub fn row_wrapped<R>(
    ui: &mut egui::Ui,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    row_with(ui, true, Box::new(add_contents))
}

fn row_with<'c, R>(
    ui: &mut egui::Ui,
    wrap: bool,
    add_contents: Box<dyn FnOnce(&mut egui::Ui) -> R + 'c>,
) -> egui::InnerResponse<R> {
    let id = ui.auto_id_with("proava_row_height");
    let minimum = ui.spacing().interact_size.y;
    let known = ui
        .data(|data| data.get_temp::<f32>(id))
        .unwrap_or(minimum)
        .max(minimum);
    let layout = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(wrap);
    let size = egui::vec2(ui.available_size_before_wrap().x, known);
    let response = ui.allocate_ui_with_layout(size, layout, add_contents);
    let height = response.response.rect.height();
    // Every line is at least `known` high: below two lines it is one line,
    // and its height is that of the tallest item.
    let one_line = !wrap || height < 2.0 * known + ui.spacing().item_spacing.y - 0.5;
    if one_line && height > known + 0.5 {
        ui.data_mut(|data| data.insert_temp(id, height));
        ui.ctx().request_discard("Zeilenhöhe");
    }
    response
}

/// A primary (accent) or normal button with a minimum size.
pub fn button(
    ui: &mut egui::Ui,
    palette: &Palette,
    label: &str,
    primary: bool,
    width: f32,
) -> egui::Response {
    let text = RichText::new(label).size(13.0).color(if primary {
        palette.on_accent
    } else {
        palette.text
    });
    let mut button = egui::Button::new(text).min_size(egui::vec2(width, 30.0));
    if primary {
        button = button.fill(palette.accent);
    }
    ui.add(button)
}

/// A row of buttons aligned right that is only as high as the buttons
/// (a plain right-to-left layout would fill the rest of a dialog).
pub fn button_row<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 32.0),
        egui::Layout::right_to_left(egui::Align::Center),
        add_contents,
    )
    .inner
}
