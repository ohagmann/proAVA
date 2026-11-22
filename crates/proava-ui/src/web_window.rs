//! proAVA's own browser window for AUSSCHREIBEN.DE and STLB-Bau Online.
//!
//! The window is a separate program (`proava-web`, wry with the system's
//! browser engine) so that proAVA itself stays pure Rust. proAVA starts it
//! through the brake in [`crate::jobs::start_helper`] and talks to it line by
//! line ([`ToHelper`] on its input, [`FromHelper`] on its output). Downloads
//! the user makes on the page (GAEB, zip) are reported here and taken over by
//! the take-over dialog like a file from the Downloads folder. Without the
//! helper (not installed, setting „System-Browser“) the system browser and
//! the Downloads folder do the job.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use proava_web_protocol::{decode, encode, FromHelper, Launch, Site, ToHelper};

use crate::jobs::{self, HelperProcess};

/// A running browser window.
pub struct WebWindow {
    site: Site,
    process: HelperProcess,
    events: Receiver<FromHelper>,
    open: bool,
}

impl WebWindow {
    /// Starts the window on `url` (a page of the site, else its start page)
    /// with „Ziel im LV“ `target`. `Ok(None)`: nothing started (tests).
    pub fn start(
        ctx: &egui::Context,
        site: Site,
        url: Option<String>,
        target: &str,
    ) -> Result<Option<Self>, String> {
        let launch = Launch {
            url,
            target: (!target.is_empty()).then(|| target.to_string()),
            ..Launch::new(site)
        };
        let Some(mut process) = jobs::start_helper(&launch.arguments())? else {
            return Ok(None);
        };
        let (sender, events) = mpsc::channel();
        if let Some(output) = process.output.take() {
            let ctx = ctx.clone();
            std::thread::Builder::new()
                .name("proava-web-meldungen".into())
                .spawn(move || {
                    for line in BufReader::new(output).lines() {
                        let Ok(line) = line else {
                            break;
                        };
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Ok(event) = decode::<FromHelper>(&line) {
                            if sender.send(event).is_err() {
                                return;
                            }
                            ctx.request_repaint();
                        }
                    }
                    // The window ended (closed, crashed): tell the dialog.
                    let _ = sender.send(FromHelper::Closed);
                    ctx.request_repaint();
                })
                .map_err(|error| format!("Browserfenster: {error}"))?;
        }
        Ok(Some(Self {
            site,
            process,
            events,
            open: true,
        }))
    }

    pub fn site(&self) -> Site {
        self.site
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Sends one message; a closed window is no error (it ends the window).
    pub fn send(&mut self, message: &ToHelper) {
        let sent = self.process.input.as_mut().is_some_and(|input| {
            writeln!(input, "{}", encode(message)).is_ok() && input.flush().is_ok()
        });
        if !sent {
            self.open = false;
        }
    }

    /// Opens a page of the site in the window and brings it to the front.
    pub fn open_page(&mut self, url: &str) {
        self.send(&ToHelper::Open {
            url: url.to_string(),
        });
    }

    /// Messages since the last call; after [`FromHelper::Closed`] the
    /// window is gone.
    pub fn poll(&mut self) -> Vec<FromHelper> {
        let events: Vec<FromHelper> = self.events.try_iter().collect();
        if events.contains(&FromHelper::Closed) {
            self.open = false;
        }
        events
    }
}

/// The site for a text source of the take-over dialog.
pub fn site_of(source: crate::stlb_dialog::TextSource) -> Option<Site> {
    match source {
        crate::stlb_dialog::TextSource::AusschreibenDe => Some(Site::Ausschreiben),
        crate::stlb_dialog::TextSource::StlbBau => Some(Site::StlbBau),
        crate::stlb_dialog::TextSource::GaebFile => None,
    }
}

/// Where the pages of a text source open: proAVA's window (`Ok`) when it is
/// installed and not switched off in the settings, else the system browser
/// with the reason (`Err`), which the dialog shows – never silently.
pub fn own_window(system_browser_chosen: bool, helper: Result<(), String>) -> Result<(), String> {
    if system_browser_chosen {
        return Err(
            "so eingestellt unter Extras › Einstellungen › Neue Projekte › Textquellen.".into(),
        );
    }
    helper
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_window_only_when_installed_and_chosen() {
        assert_eq!(own_window(false, Ok(())), Ok(()));
        let chosen = own_window(true, Ok(())).unwrap_err();
        assert!(
            chosen.contains("Einstellungen"),
            "System-Browser gewählt: {chosen}"
        );
        assert_eq!(
            own_window(false, Err("proava-web fehlt".into())),
            Err("proava-web fehlt".into()),
            "nicht installiert: Grund bleibt"
        );
        assert_eq!(
            site_of(crate::stlb_dialog::TextSource::AusschreibenDe),
            Some(Site::Ausschreiben)
        );
        assert_eq!(
            site_of(crate::stlb_dialog::TextSource::StlbBau),
            Some(Site::StlbBau)
        );
        assert_eq!(site_of(crate::stlb_dialog::TextSource::GaebFile), None);
    }

    /// Tests never start the window (no test starts a process).
    #[test]
    fn tests_never_start_the_window() {
        let ctx = egui::Context::default();
        let started = WebWindow::start(&ctx, Site::Ausschreiben, None, "Ziel").unwrap();
        assert!(started.is_none());
    }
}
