//! Dragging rows from one proAVA window into another.
//!
//! Every window is a process of its own, and winit cannot start a drag
//! across processes (no XDND/OLE source). But while the mouse button is held,
//! the system keeps sending the pointer to the window where the drag began –
//! X11 through the implicit pointer grab, Windows through the mouse capture
//! winit sets on a button press – also beyond its edge (egui keeps the drag
//! going, `PointerGone` is not a release). So the window of the drag works
//! out the point on the desktop (its inside on the desktop plus the pointer,
//! in physical pixels), finds the window under it in the folder of the local
//! bridge and tells it over the bridge: `ziehen_ueber` (mark the place),
//! `ziehen_ende`, `ablegen` (insert there, the way of Strg+V). Under Wayland
//! the system does not tell where a window is: no drag between windows, a
//! hint names Strg+C/Strg+V instead.
//!
//! The messages go out on a helper thread (the UI thread never waits for
//! another window); movements are bundled – only the newest position of a
//! run is sent.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use proava_application::window_bridge::{Messenger, Wake};
use proava_bridge_protocol::{Command, Reply, Request, WindowEntry};

/// At most this often a new position goes to the other window (seconds).
pub const OVER_EVERY: f64 = 1.0 / 30.0;
/// The position is sent again after this time even without movement, so the
/// other window keeps its mark and scrolls along (seconds).
pub const OVER_REPEAT: f64 = 0.2;
/// The other window forgets a drag it has not heard of for this long.
pub const OVER_FORGET: f64 = 3.0;
/// The folder of the windows is read again after this time during a drag.
pub const ENTRIES_EVERY: f64 = 0.25;

/// What the helper thread sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Over,
    End,
    Drop,
}

struct Job {
    kind: Kind,
    channel: String,
    request: Request,
}

/// One answer of the other window.
pub struct Answer {
    pub kind: Kind,
    pub result: Result<Reply, String>,
}

/// The helper thread of one drag: it ends when the drag (its sender) is gone
/// and everything queued is sent.
pub struct Messages {
    jobs: Sender<Job>,
    answers: Receiver<Answer>,
    next: u64,
}

impl Messages {
    pub fn start(messenger: Messenger, wake: Wake) -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (outbox, answers) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("proava-ziehen".into())
            .spawn(move || {
                while let Ok(first) = inbox.recv() {
                    for job in bundle(first, inbox.try_iter()) {
                        let timeout = match job.kind {
                            Kind::Over | Kind::End => Duration::from_secs(2),
                            // Letting go may meet a question of proAVA in the other window.
                            Kind::Drop => Duration::from_secs(75),
                        };
                        let result = messenger(&job.channel, &job.request, timeout)
                            .map(|response| response.antwort);
                        if outbox
                            .send(Answer {
                                kind: job.kind,
                                result,
                            })
                            .is_err()
                        {
                            return;
                        }
                        wake();
                    }
                }
            });
        Self {
            jobs,
            answers,
            next: 1,
        }
    }

    pub fn send(&mut self, kind: Kind, window: &WindowEntry, command: Command) {
        let request = Request::new(self.next, command);
        self.next += 1;
        let _ = self.jobs.send(Job {
            kind,
            channel: window.kanal.clone(),
            request,
        });
    }

    /// Answers that came in since the last look.
    pub fn answers(&self) -> Vec<Answer> {
        self.answers.try_iter().collect()
    }
}

/// Of a run of position messages to the same window only the newest goes
/// out; ends and drops keep their order.
fn bundle(first: Job, more: impl Iterator<Item = Job>) -> Vec<Job> {
    let mut list: Vec<Job> = Vec::new();
    for job in std::iter::once(first).chain(more) {
        if job.kind == Kind::Over {
            if let Some(last) = list.last() {
                if last.kind == Kind::Over && last.channel == job.channel {
                    list.pop();
                }
            }
        }
        list.push(job);
    }
    list
}

/// The point on the desktop in physical pixels: the inside of the window on
/// the desktop (points) plus the pointer (points of the window), times the
/// pixels per point.
pub fn desktop_point(inside: egui::Rect, pointer: egui::Pos2, pixels_per_point: f32) -> [f32; 2] {
    let point = (inside.min + pointer.to_vec2()) * pixels_per_point;
    [point.x, point.y]
}

/// A point on the desktop (physical pixels) in points of this window.
pub fn window_point(inside: egui::Rect, desktop: [f32; 2], pixels_per_point: f32) -> egui::Pos2 {
    egui::pos2(
        desktop[0] / pixels_per_point - inside.min.x,
        desktop[1] / pixels_per_point - inside.min.y,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn job(kind: Kind, channel: &str, id: u64) -> Job {
        Job {
            kind,
            channel: channel.into(),
            request: Request::new(id, Command::FensterLesen),
        }
    }

    #[test]
    fn positions_are_bundled_ends_and_drops_kept() {
        let list = bundle(
            job(Kind::Over, "b", 1),
            vec![
                job(Kind::Over, "b", 2),
                job(Kind::Over, "b", 3),
                job(Kind::End, "b", 4),
                job(Kind::Over, "c", 5),
                job(Kind::Over, "c", 6),
                job(Kind::Drop, "c", 7),
            ]
            .into_iter(),
        );
        let ids: Vec<u64> = list.iter().map(|job| job.request.id).collect();
        assert_eq!(ids, [3, 4, 6, 7]);
    }

    #[test]
    fn points_between_window_and_desktop() {
        let inside = egui::Rect::from_min_size(egui::pos2(960.0, 40.0), egui::vec2(900.0, 700.0));
        let desktop = desktop_point(inside, egui::pos2(-100.0, 20.0), 1.5);
        assert_eq!(desktop, [1290.0, 90.0]);
        let back = window_point(inside, desktop, 1.5);
        assert!((back.x + 100.0).abs() < 1e-3 && (back.y - 20.0).abs() < 1e-3);
    }

    #[test]
    fn messages_go_out_on_a_helper_thread() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = sent.clone();
        let messenger: Messenger = Arc::new(move |channel: &str, request: &Request, _| {
            log.lock().unwrap().push((channel.to_string(), request.id));
            Ok(proava_bridge_protocol::Response::new(
                request.id,
                Reply::refused("gesehen"),
            ))
        });
        let mut messages = Messages::start(messenger, Arc::new(|| {}));
        let window = WindowEntry {
            kanal: "/x/b.sock".into(),
            ..WindowEntry::default()
        };
        messages.send(Kind::Over, &window, Command::FensterLesen);
        messages.send(Kind::Drop, &window, Command::FensterLesen);
        let mut answers = Vec::new();
        for _ in 0..200 {
            answers.extend(messages.answers());
            if answers.len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[1].kind, Kind::Drop);
        assert!(sent
            .lock()
            .unwrap()
            .iter()
            .all(|(channel, _)| channel == "/x/b.sock"));
    }
}
