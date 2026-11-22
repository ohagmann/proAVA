//! Build information for „Über proAVA“: Git commit and build date.
//! Without Git (e.g. a source archive) the values stay „unbekannt“.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|text| !text.is_empty())
}

fn main() {
    let commit = git(&["rev-parse", "--short=10", "HEAD"]).unwrap_or_else(|| "unbekannt".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|status| !status.is_empty())
        .unwrap_or(false);
    let date = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|epoch| epoch.parse::<i64>().ok())
        .map(day_of)
        .unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            day_of(now)
        });
    println!(
        "cargo:rustc-env=PROAVA_GIT_COMMIT={commit}{}",
        if dirty { "+geändert" } else { "" }
    );
    println!("cargo:rustc-env=PROAVA_BUILD_DATE={date}");
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(index) = git(&["rev-parse", "--git-path", "index"]) {
        println!("cargo:rerun-if-changed={index}");
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rerun-if-changed=build.rs");
}

/// Seconds since 1970 → `TT.MM.JJJJ` (UTC), civil calendar without extra crates.
fn day_of(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day:02}.{month:02}.{year}")
}
