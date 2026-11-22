//! Free memory before a big printout: an LV of 2000 positions needs about
//! 2 GB while Typst lays it out. A printout that clearly does not fit into the
//! free memory is refused with a message instead of making the system swap or
//! the program crash (office laptops have 8–16 GB). Nothing is refused when
//! the free memory is unknown or the estimate fits.

use proava_print::PrintEstimate;

/// Memory a printout may use now, in bytes: what the system reports as
/// available and, on Linux, the room left under the memory limits of the own
/// control group (systemd scope, container). None when unknown.
pub fn available_memory() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let system = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|text| meminfo_available(&text));
        let group = cgroup_room();
        match (system, group) {
            (Some(system), Some(group)) => Some(system.min(group)),
            (system, group) => system.or(group),
        }
    }
    #[cfg(windows)]
    {
        windows_available()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// `MemAvailable` of /proc/meminfo in bytes.
#[cfg(any(target_os = "linux", test))]
fn meminfo_available(text: &str) -> Option<u64> {
    let line = text
        .lines()
        .find(|line| line.starts_with("MemAvailable:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Smallest room `memory.max − memory.current` of the own cgroup (v2) and its
/// parents; None without a limit.
#[cfg(target_os = "linux")]
fn cgroup_room() -> Option<u64> {
    let membership = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))?;
    let mut folder = std::path::PathBuf::from("/sys/fs/cgroup").join(path.trim_start_matches('/'));
    let mut room: Option<u64> = None;
    loop {
        let read = |name: &str| std::fs::read_to_string(folder.join(name)).ok();
        if let (Some(max), Some(current)) = (read("memory.max"), read("memory.current")) {
            if let (Ok(max), Ok(current)) =
                (max.trim().parse::<u64>(), current.trim().parse::<u64>())
            {
                let free = max.saturating_sub(current);
                room = Some(room.map_or(free, |room| room.min(free)));
            }
        }
        if folder == std::path::Path::new("/sys/fs/cgroup") || !folder.pop() {
            break;
        }
    }
    room
}

#[cfg(windows)]
fn windows_available() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    // SAFETY: `status` is a properly sized MEMORYSTATUSEX with dwLength set,
    // as the call requires; it lives for the duration of the call.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    ok.then_some(status.ullAvailPhys)
}

fn gigabytes(bytes: u64) -> String {
    let value = bytes as f64 / 1_073_741_824.0;
    format!("{value:.1}").replace('.', ",")
}

/// Refuses a printout whose estimated peak is larger than the free memory
/// (`available`, None = unknown: no refusal).
pub fn memory_check(estimate: PrintEstimate, available: Option<u64>) -> Result<(), String> {
    let Some(available) = available else {
        return Ok(());
    };
    if estimate.peak_bytes <= available {
        return Ok(());
    }
    let mut message = format!(
        "{} (rund {} Seiten) und braucht voraussichtlich rund {} GB Arbeitsspeicher, \
         frei sind nur {} GB. Bitte andere Programme schließen oder nur einen Teil drucken \
         (Bereich „markierte Titel“).",
        proava_application::MEMORY_REFUSAL,
        estimate.pages,
        gigabytes(estimate.peak_bytes),
        gigabytes(available)
    );
    if let Some(without) = estimate.peak_without_carry_bytes {
        message.push_str(&format!(
            " Ohne „Übertrag“ braucht er rund {} GB.",
            gigabytes(without)
        ));
    }
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_073_741_824;

    fn estimate(pages: usize, peak_bytes: u64) -> PrintEstimate {
        PrintEstimate {
            pages,
            peak_bytes,
            peak_without_carry_bytes: None,
        }
    }

    #[test]
    fn a_printout_that_fits_or_unknown_memory_is_never_refused() {
        assert!(memory_check(estimate(500, 2 * GB), Some(8 * GB)).is_ok());
        assert!(memory_check(estimate(500, 2 * GB), Some(2 * GB)).is_ok());
        assert!(memory_check(estimate(5000, 20 * GB), None).is_ok());
    }

    #[test]
    fn a_printout_that_does_not_fit_gets_a_clear_message() {
        let error = memory_check(estimate(506, 2 * GB + GB / 5), Some(GB + GB / 2)).unwrap_err();
        assert!(error.contains("rund 506 Seiten"), "{error}");
        assert!(error.contains("rund 2,2 GB"), "{error}");
        assert!(error.contains("frei sind nur 1,5 GB"), "{error}");
        assert!(error.contains("markierte Titel"), "{error}");
        assert!(!error.contains('!'), "{error}");
        assert!(!error.contains("Übertrag"), "{error}");
        assert!(proava_application::is_memory_refusal(&error));
        assert!(!proava_application::is_memory_refusal(
            "Für den Preisspiegel fehlt ein gewertetes Angebot."
        ));
    }

    #[test]
    fn with_carry_the_message_says_what_the_printout_needs_without() {
        let mut big = estimate(506, 2 * GB + GB / 5);
        big.peak_without_carry_bytes = Some(GB + GB / 3);
        let error = memory_check(big, Some(GB + GB / 2)).unwrap_err();
        assert!(
            error.ends_with("Ohne „Übertrag“ braucht er rund 1,3 GB."),
            "{error}"
        );
    }

    #[test]
    fn meminfo_is_read_in_bytes() {
        let text = "MemTotal:       131000000 kB\nMemFree:  1000 kB\nMemAvailable:    8388608 kB\n";
        assert_eq!(meminfo_available(text), Some(8 * GB));
        assert_eq!(meminfo_available("MemTotal: 5 kB\n"), None);
    }

    /// On this system the free memory is known (Linux, Windows).
    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn free_memory_is_known() {
        let available = available_memory().expect("freier Speicher");
        assert!(available > 0);
    }
}
