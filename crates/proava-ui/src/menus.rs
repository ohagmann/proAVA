//! Menüs, die nie über die Menüleiste ragen (Anwender 18.11.): Ein Menü öffnet
//! direkt unter seinem Eintrag in der Menüleiste. Ist es höher als der Platz
//! bis zum Fensterrand, wird es abgeschnitten und scrollt – wie unter Windows
//! und macOS:
//!
//! - unten ein Pfeil, solange weitere Einträge folgen, oben einer, sobald
//!   gescrollt ist; Überfahren eines Pfeils scrollt gleichmäßig,
//! - das Mausrad scrollt,
//! - mit den Pfeiltasten folgt die Liste dem gewählten Eintrag.
//!
//! [`bar`] ersetzt `egui::MenuBar` (merkt sich die Unterkante der Leiste),
//! [`menu`] ersetzt `ui.menu_button` für Menüs und Untermenüs, [`scroll`]
//! nimmt den Inhalt eines langen Kontextmenüs auf.

use eframe::egui::{self, Id, Rect, Sense, Vec2};

/// Height of the strip with the scroll arrow at the top and bottom.
pub const ARROW_HEIGHT: f32 = 16.0;
/// Scroll speed while the pointer rests on an arrow, in points per second.
pub const HOVER_SPEED: f32 = 240.0;
/// Free room kept between a menu and the lower window edge.
const EDGE: f32 = 6.0;
/// A menu shows at least this much even in a very low window.
const MIN_HEIGHT: f32 = 120.0;

/// Where the menu bar ends (stored by [`bar`], read by every menu).
fn bar_bottom_id() -> Id {
    Id::unique("proava_menu_bar_bottom")
}

/// A click on a scroll arrow: the menu (popup id) stays open (egui closes a
/// menu on every click inside it).
fn keep_open_id() -> Id {
    Id::unique("proava_menu_keep_open")
}

/// What one scrolling menu remembers between frames.
#[derive(Debug, Clone, Copy, Default)]
struct Memo {
    /// Height of all entries.
    content: f32,
    /// Current offset and its maximum.
    offset: f32,
    max_offset: f32,
    /// The pointer rests on the upper / lower arrow.
    up: bool,
    down: bool,
    /// The entry that had the keyboard focus (to follow it once).
    focus: Option<Id>,
    /// Pass in which the menu was shown last (a new opening starts at the top).
    pass: u64,
}

/// The menu bar: like `egui::MenuBar`, but it remembers its lower edge, so
/// that no menu reaches above it.
pub fn bar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let response = egui::MenuBar::new().ui(ui, add);
    let bottom = response.response.rect.bottom();
    ui.ctx()
        .data_mut(|data| data.insert_temp(bar_bottom_id(), bottom));
    response.inner
}

/// A menu of the menu bar or a submenu (`ui.menu_button`) whose entries
/// scroll when the window is too low.
pub fn menu<'a, R>(
    ui: &mut egui::Ui,
    label: impl egui::IntoAtoms<'a>,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let response = ui.menu_button(label, |ui| scroll(ui, add));
    let popup = egui::Popup::default_response_id(&response.response);
    let ctx = ui.ctx();
    if ctx.data(|data| data.get_temp::<Id>(keep_open_id())) == Some(popup) {
        ctx.data_mut(|data| data.remove::<Id>(keep_open_id()));
        egui::Popup::open_id(ctx, popup);
    }
    response
}

/// The height the entries of a menu may take: from the menu bar (or the top
/// of the window) to the lower window edge, without the menu's frame.
pub fn room(ctx: &egui::Context, style: &egui::Style) -> f32 {
    let window = ctx.content_rect();
    let top = ctx
        .data(|data| data.get_temp::<f32>(bar_bottom_id()))
        .unwrap_or(window.top())
        .clamp(window.top(), window.bottom());
    let frame = style.spacing.menu_margin.sum().y + 2.0 * style.visuals.window_stroke().width;
    (window.bottom() - top - frame - EDGE).max(MIN_HEIGHT)
}

/// The entries of a menu, scrolling with arrows when they do not fit
/// ([`room`]). Short menus look and behave as before.
pub fn scroll<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let id = ui.layer_id().id.with("proava_menu_scroll");
    let mut memo: Memo = ctx.data(|data| data.get_temp(id)).unwrap_or_default();
    let room = room(&ctx, ui.style());
    let overflow = memo.content > room + 0.5;
    let view = if overflow {
        room - 2.0 * ARROW_HEIGHT
    } else {
        room
    };

    // Upper arrow strip (filled in below, once the width is known).
    let top_strip = overflow.then(|| ui.allocate_space(Vec2::new(0.0, ARROW_HEIGHT)).1);

    let mut area = egui::ScrollArea::vertical()
        .id_salt(id)
        .max_height(view)
        // The popup offers only the height of its last size (in its first
        // frame a fixed default): ask for the full room, shrink to the entries.
        .min_scrolled_height(view)
        .auto_shrink([true, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL);
    let pass = ctx.cumulative_pass_nr();
    if memo.pass + 1 < pass {
        // Opened anew: from the top, as in every menu.
        memo.up = false;
        memo.down = false;
        memo.offset = 0.0;
        memo.focus = None;
        area = area.vertical_scroll_offset(0.0);
    } else if overflow && (memo.up || memo.down) {
        let step = HOVER_SPEED * ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
        let wanted = if memo.down {
            memo.offset + step
        } else {
            memo.offset - step
        };
        let wanted = wanted.clamp(0.0, memo.max_offset);
        if wanted != memo.offset {
            area = area.vertical_scroll_offset(wanted);
            ctx.request_repaint();
        }
    }
    let previous_focus = memo.focus;
    let layer = ui.layer_id();
    let output = area.show(ui, |ui| {
        let inner = add(ui);
        // Arrow keys move the focus; the list follows the chosen entry.
        let focused = ui.memory(|memory| memory.focused());
        if focused != previous_focus {
            let response = focused.and_then(|focused| ui.ctx().read_response(focused));
            if let Some(response) = response.filter(|response| response.layer_id == layer) {
                ui.scroll_to_rect(response.rect, None);
            }
        }
        (inner, focused)
    });
    let (inner, focused) = output.inner;
    let bottom_strip = overflow.then(|| ui.allocate_space(Vec2::new(0.0, ARROW_HEIGHT)).1);

    memo.content = output.content_size.y;
    memo.offset = output.state.clamped_offset().y;
    memo.max_offset = (output.content_size.y - output.inner_rect.height()).max(0.0);
    memo.focus = focused;
    memo.pass = pass;
    memo.up = false;
    memo.down = false;
    let full = ui.min_rect().x_range();
    let color = ui.visuals().text_color();
    // The arrows react to the pointer only, not to Tab or the arrow keys.
    if let Some(strip) = top_strip {
        let strip = Rect::from_x_y_ranges(full, strip.y_range());
        let can = memo.offset > 0.5;
        let response = ui.interact(strip, id.with("up"), Sense::CLICK);
        let response = arrow_name(ui, response, "Nach oben scrollen");
        if response.clicked() {
            ctx.data_mut(|data| data.insert_temp(keep_open_id(), ui.layer_id().id));
        }
        if can {
            crate::icons::scroll_arrow(ui.painter(), strip.center(), true, color);
            memo.up = response.hovered();
        }
    }
    if let Some(strip) = bottom_strip {
        let strip = Rect::from_x_y_ranges(full, strip.y_range());
        let can = memo.offset < memo.max_offset - 0.5;
        let response = ui.interact(strip, id.with("down"), Sense::CLICK);
        let response = arrow_name(ui, response, "Nach unten scrollen");
        if response.clicked() {
            ctx.data_mut(|data| data.insert_temp(keep_open_id(), ui.layer_id().id));
        }
        if can {
            crate::icons::scroll_arrow(ui.painter(), strip.center(), false, color);
            memo.down = response.hovered();
        }
    }
    if memo.up || memo.down {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| data.insert_temp(id, memo));
    inner
}

/// Name for screen readers and UI tests.
fn arrow_name(ui: &egui::Ui, response: egui::Response, name: &str) -> egui::Response {
    let enabled = ui.is_enabled();
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, name));
    response
}
