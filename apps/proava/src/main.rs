//! Composition root: wires storage, use cases and the start screen together.
//!
//! This is the only place that knows which concrete file formats back the ports.
//!
//! Command line: `proava [DATEI]` – a `.proava` project opens directly, a
//! GAEB LV (X81–X86, D81–D86) starts „Neues Projekt aus GAEB“ with it.
//! `proava --schreibgeschuetzt Projekt.proava` opens it read-only (second
//! window to copy from). `proava --version` prints the version.

// Windows release builds start without a console window; debug builds keep it for log output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use proava_application::Launcher;
use proava_exchange::GaebPdfExchange;
use proava_infrastructure::{CurlWebTexts, DiskFolders, JsonSettingsFile, XmlProjectFile};
use proava_ui::about;
use proava_ui::StartFile;

fn main() -> eframe::Result<()> {
    let mut start = None;
    let mut read_only = false;
    for argument in std::env::args_os().skip(1) {
        match argument.to_str() {
            // Second window to copy from: the project opens without saving.
            Some("--schreibgeschuetzt" | "--read-only") => {
                read_only = true;
                continue;
            }
            Some("--version" | "-V") => {
                proava_ui::attach_parent_console();
                println!(
                    "proAVA {} (Build {}, Commit {})",
                    about::VERSION,
                    about::BUILD_DATE,
                    about::GIT_COMMIT
                );
                return Ok(());
            }
            Some("--help" | "-h") => {
                proava_ui::attach_parent_console();
                println!(
                    "Aufruf: proava [--schreibgeschuetzt] [Projekt.proava | GAEB-Datei .X81–.X86/.D81–.D86 | CSV-Tabelle .csv]"
                );
                return Ok(());
            }
            // Desktop environments may pass a file:// URI (%u).
            Some(text) if text.starts_with("file://") => {
                start = StartFile::of(absolute(file_uri_path(
                    &text["file://".len()..],
                    cfg!(windows),
                )));
            }
            _ => start = StartFile::of(absolute(PathBuf::from(argument))),
        }
        if start.is_none() {
            proava_ui::attach_parent_console();
            eprintln!("proAVA: Datei wird nicht unterstützt (erwartet .proava, GAEB .X8x/.D8x oder .csv).");
        }
    }
    if read_only {
        if let Some(StartFile::Project(path)) = start {
            start = Some(StartFile::ProjectReadOnly(path));
        }
    }

    let launcher = Arc::new(
        Launcher::new(
            Box::new(XmlProjectFile::new()),
            Box::new(JsonSettingsFile::new()),
            Box::new(DiskFolders::new()),
        )
        .with_exchange(Box::new(GaebPdfExchange))
        .with_web(Box::new(CurlWebTexts::new())),
    );

    // Local bridge for assistants (MCP server proava-mcp): only for this
    // user, only this program – never in tests.
    let channel = proava_infrastructure::window_bridge::SocketChannel::standard().map(|channel| {
        Box::new(channel) as Box<dyn proava_application::window_bridge::WindowChannel>
    });
    proava_ui::run_with_bridge(launcher, start, channel)
}

/// A path relative to the current folder (`proava LV.X83` in a console)
/// as an absolute one, so „Zuletzt verwendet“ finds the project again from
/// anywhere. Windows: `GetFullPathNameW`, no `\\?\` prefix; nothing is
/// resolved on the disk.
fn absolute(path: PathBuf) -> PathBuf {
    std::path::absolute(&path).unwrap_or(path)
}

/// The path of a `file://` URI (without the scheme): `%20` decoded; on
/// Windows `/C:/LV.X83` becomes `C:/LV.X83`.
fn file_uri_path(rest: &str, windows: bool) -> PathBuf {
    let path = percent_decode(rest);
    let bytes = path.as_bytes();
    if windows
        && bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
    {
        return PathBuf::from(&path[1..]);
    }
    PathBuf::from(path)
}

/// `%20` → space etc. in a file URI (UTF-8 bytes).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(value) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or(())
            {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_uri_is_decoded() {
        assert_eq!(
            super::percent_decode("/home/nutzer/Gemeinsame%20Dokumente/LV%C3%A4.X83"),
            "/home/nutzer/Gemeinsame Dokumente/LVä.X83"
        );
        assert_eq!(super::percent_decode("/a/b%2"), "/a/b%2");
    }

    #[test]
    fn relative_start_files_become_absolute() {
        use std::path::PathBuf;
        let path = super::absolute(PathBuf::from("Neubau Schule.proava"));
        assert!(path.is_absolute(), "{}", path.display());
        assert!(path.ends_with("Neubau Schule.proava"));
        assert_eq!(
            super::absolute(path.clone()),
            path,
            "absolute Pfade bleiben"
        );
    }

    #[test]
    fn file_uri_with_drive_letter_on_windows() {
        use std::path::PathBuf;
        assert_eq!(
            super::file_uri_path("/C:/LV%20neu.X83", true),
            PathBuf::from("C:/LV neu.X83")
        );
        assert_eq!(
            super::file_uri_path("/home/nutzer/a.X83", true),
            PathBuf::from("/home/nutzer/a.X83")
        );
        assert_eq!(
            super::file_uri_path("/C:/a.X83", false),
            PathBuf::from("/C:/a.X83")
        );
    }
}
