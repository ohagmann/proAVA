//! Web access for the published catalogue lists (port `WebTexts`).
//!
//! proAVA has no HTTP/TLS stack of its own: the lists are fetched with
//! `curl` (Linux and macOS have it, Windows 10 and later bring `curl.exe`),
//! HTTPS only, with a time limit and a plain user agent, and kept in the
//! cache folder. A fresh copy is used without asking the server; when the
//! server cannot be reached an older copy is better than nothing.
//!
//! Tests never start curl (no test starts a process): they hand
//! in their own download function ([`CurlWebTexts::with_download`]).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use proava_application::WebTexts;

/// User agent of proAVA's requests (no personal data).
pub const USER_AGENT: &str = "proAVA (internes AVA-Programm; Katalogverzeichnisse)";
/// Seconds per request.
const TIMEOUT_SECONDS: &str = "30";

/// Fetches the text at an address (curl in the program, a stand-in in tests).
type Download = Box<dyn Fn(&str) -> Result<String, String> + Send + Sync>;

pub struct CurlWebTexts {
    cache: PathBuf,
    download: Download,
}

impl CurlWebTexts {
    /// Cache in the program's cache folder (`~/.cache/proava/texte`), curl.
    pub fn new() -> Self {
        Self::with_download(
            crate::paths::cache_dir().join("texte"),
            Box::new(|url| curl(&curl_program(), url)),
        )
    }

    /// Own cache folder and download function (tests).
    pub fn with_download(cache: PathBuf, download: Download) -> Self {
        Self { cache, download }
    }

    fn cache_file(&self, url: &str) -> PathBuf {
        self.cache
            .join(format!("{:016x}.txt", fnv1a(url.as_bytes())))
    }
}

impl Default for CurlWebTexts {
    fn default() -> Self {
        Self::new()
    }
}

impl WebTexts for CurlWebTexts {
    fn get_text(&self, url: &str, max_age_days: u32) -> Result<String, String> {
        if !url.starts_with("https://") {
            return Err(format!("Nur HTTPS: {url}"));
        }
        let file = self.cache_file(url);
        let max_age = Duration::from_secs(u64::from(max_age_days) * 24 * 3600);
        if is_younger(&file, max_age) {
            if let Ok(text) = std::fs::read_to_string(&file) {
                return Ok(text);
            }
        }
        match (self.download)(url) {
            Ok(text) => {
                // The cache is a convenience: failing to write it is no error.
                let _ = store(&file, &text);
                Ok(text)
            }
            Err(error) => std::fs::read_to_string(&file).map_err(|_| error),
        }
    }
}

/// Arguments for curl: HTTPS only (also after redirects), fail on HTTP
/// errors, time limit, plain user agent, the address after `--`.
fn curl_arguments(url: &str) -> Vec<&str> {
    vec![
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-time",
        TIMEOUT_SECONDS,
        "--user-agent",
        USER_AGENT,
        "--",
        url,
    ]
}

/// The curl to start. Windows: the one of the system
/// (`%SystemRoot%\System32\curl.exe`) – Windows looks for a bare „curl“ in
/// the program folder first, where any `curl.exe` could lie (e.g. a zip
/// unpacked into Downloads); elsewhere „curl“ from the PATH.
fn curl_program() -> PathBuf {
    #[cfg(windows)]
    if let Some(system) = std::env::var_os("SystemRoot") {
        let system_curl = PathBuf::from(system).join("System32").join("curl.exe");
        if system_curl.is_file() {
            return system_curl;
        }
    }
    PathBuf::from("curl")
}

fn curl(program: &Path, url: &str) -> Result<String, String> {
    let mut command = Command::new(program);
    command.args(curl_arguments(url));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window from the GUI program.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|error| {
        format!("curl lässt sich nicht starten ({error}) – ohne curl keine Suche, der Weg über den Browser bleibt.")
    })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("{url} nicht erreichbar: {message}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn is_younger(file: &Path, max_age: Duration) -> bool {
    std::fs::metadata(file)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < max_age)
}

/// Writes next to the target and renames (no half file in the cache).
fn store(file: &Path, text: &str) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let partial = file.with_extension("part");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, file)
}

/// FNV-1a 64 – file name of a cached address.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use super::*;

    fn folder(name: &str) -> PathBuf {
        // Unique and created exclusively (also against other test runs at the
        // same time); the test removes it.
        let folder = tempfile::Builder::new()
            .prefix(&format!("proava-web-{name}-"))
            .tempdir()
            .unwrap()
            .keep();
        // The code under test creates it.
        std::fs::remove_dir_all(&folder).ok();
        folder
    }

    /// A stand-in for curl that counts its calls and answers or fails.
    fn stand_in(answer: Option<&'static str>, calls: Arc<AtomicUsize>) -> Download {
        Box::new(move |url| {
            calls.fetch_add(1, Ordering::SeqCst);
            answer
                .map(|text| format!("{text} {url}"))
                .ok_or_else(|| "nicht erreichbar".to_string())
        })
    }

    #[test]
    fn a_fresh_copy_is_used_an_old_one_only_when_the_server_fails() {
        let cache = folder("cache");
        let calls = Arc::new(AtomicUsize::new(0));
        let web =
            CurlWebTexts::with_download(cache.clone(), stand_in(Some("# Liste"), calls.clone()));
        let url = "https://ausschreiben.de/catalogs/llms.txt";
        let text = web.get_text(url, 7).unwrap();
        assert_eq!(text, format!("# Liste {url}"));
        assert_eq!(
            std::fs::read_to_string(web.cache_file(url)).unwrap(),
            text,
            "zwischengespeichert"
        );
        // Fresh copy: no second download.
        assert_eq!(web.get_text(url, 7).unwrap(), text);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // Old copy and the server fails: the old copy stays usable.
        let failing = Arc::new(AtomicUsize::new(0));
        let offline = CurlWebTexts::with_download(cache.clone(), stand_in(None, failing.clone()));
        let file = std::fs::File::options()
            .write(true)
            .open(offline.cache_file(url))
            .unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(30 * 24 * 3600))
            .unwrap();
        assert!(!is_younger(
            &offline.cache_file(url),
            Duration::from_secs(7 * 24 * 3600)
        ));
        assert_eq!(offline.get_text(url, 7).unwrap(), text);
        assert_eq!(failing.load(Ordering::SeqCst), 1, "Server gefragt");
        // Nothing cached and the server fails: its error.
        let error = offline
            .get_text("https://ausschreiben.de/llms.txt", 7)
            .unwrap_err();
        assert_eq!(error, "nicht erreichbar");
        assert!(
            offline.get_text("http://ausschreiben.de/x", 7).is_err(),
            "nur HTTPS"
        );
        assert_eq!(failing.load(Ordering::SeqCst), 2, "kein Abruf ohne HTTPS");
        assert_ne!(
            web.cache_file(url),
            web.cache_file("https://ausschreiben.de/llms.txt")
        );
        std::fs::remove_dir_all(&cache).ok();
    }

    #[test]
    fn curl_gets_https_only_a_plain_user_agent_and_the_address_last() {
        let url = "https://ausschreiben.de/catalog/x/llms-1.txt";
        let arguments = curl_arguments(url);
        assert_eq!(arguments.last(), Some(&url));
        assert_eq!(
            arguments[arguments.len() - 2],
            "--",
            "Adresse nie als Schalter"
        );
        let pairs: Vec<(&str, &str)> = arguments
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect();
        assert!(pairs.contains(&("--proto", "=https")));
        assert!(pairs.contains(&("--proto-redir", "=https")));
        assert!(pairs.contains(&("--user-agent", USER_AGENT)));
        assert!(pairs.contains(&("--max-time", TIMEOUT_SECONDS)));
        assert!(arguments.contains(&"--fail"));
    }
}
