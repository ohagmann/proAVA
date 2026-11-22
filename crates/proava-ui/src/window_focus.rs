//! Keyboard focus for proAVA's own windows (Positionsfenster, großes
//! Langtextfenster).
//!
//! winit asks the window manager for the focus only once the window is
//! mapped (X11: `_NET_ACTIVE_WINDOW` only with `Visibility::Yes`); the request
//! sent with the opening frame came too early and was dropped. Whether the new
//! window got the focus then depended on the window manager alone: metacity
//! gives it, Cinnamon (Muffin) with its focus stealing prevention kept it in
//! the main window – Esc and Tab went there until the user clicked into the new
//! window (18.11.2026: „ein Klick zu viel“).
//!
//! So an opened or raised window keeps asking for a short time until it has
//! the focus – never longer, so a click back into the main window wins.

use std::time::Duration;

use eframe::egui;

/// How long a window asks for the focus after opening (seconds).
const ASK_SECONDS: f64 = 1.5;

/// Pending request for the keyboard focus of one viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FocusRequest {
    /// `Some(None)`: just started, the deadline is set in the next frame.
    until: Option<Option<f64>>,
}

impl FocusRequest {
    /// Asks from now on (window opened, raised or shown again).
    pub fn start(&mut self) {
        self.until = Some(None);
    }

    /// Call each frame inside the window's own viewport (not for an embedded
    /// one): sends `ViewportCommand::Focus` until the window reports the
    /// focus or the time is up.
    pub fn keep(&mut self, ctx: &egui::Context) {
        let Some(until) = self.until else {
            return;
        };
        let (now, focused) = ctx.input(|input| (input.time, input.viewport().focused));
        let until = until.unwrap_or(now + ASK_SECONDS);
        if focused == Some(true) || now > until {
            self.until = None;
            return;
        }
        self.until = Some(Some(until));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        time: f64,
        focused: Option<bool>,
        request: &mut FocusRequest,
    ) -> bool {
        let mut input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .focused = focused;
        let mut output = ctx.run_ui(input, |ui| request.keep(ui.ctx()));
        output.textures_delta.clear();
        output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|viewport| viewport.commands.contains(&egui::ViewportCommand::Focus))
    }

    #[test]
    fn asks_until_focused() {
        let ctx = egui::Context::default();
        let mut request = FocusRequest::default();
        assert!(
            !frame(&ctx, 0.0, Some(false), &mut request),
            "nichts angefordert"
        );
        request.start();
        assert!(
            frame(&ctx, 0.1, None, &mut request),
            "Fenster noch nicht da: fragen"
        );
        assert!(
            frame(&ctx, 0.2, Some(false), &mut request),
            "Fenstermanager gab den Fokus nicht: weiter fragen"
        );
        assert!(!frame(&ctx, 0.3, Some(true), &mut request), "hat den Fokus");
        assert_eq!(request, FocusRequest::default(), "erledigt");
        assert!(
            !frame(&ctx, 0.4, Some(false), &mut request),
            "danach nie wieder (Klick ins Hauptfenster gewinnt)"
        );
    }

    #[test]
    fn gives_up_after_a_short_time() {
        let ctx = egui::Context::default();
        let mut request = FocusRequest::default();
        request.start();
        assert!(frame(&ctx, 10.0, Some(false), &mut request));
        assert!(frame(&ctx, 11.0, Some(false), &mut request));
        assert!(
            !frame(&ctx, 11.6, Some(false), &mut request),
            "nach 1,5 s Schluss"
        );
        assert_eq!(request, FocusRequest::default(), "erledigt");
    }
}
