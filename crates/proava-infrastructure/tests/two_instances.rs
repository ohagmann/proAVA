//! Two proAVA windows at the same time: the settings file must keep what
//! both remember, the lock file says who has a project open.

use std::path::{Path, PathBuf};

use proava_application::{Launcher, ProjectRepository};
use proava_domain::{Lv, OzMask, Project, ProjectName};
use proava_infrastructure::{DiskFolders, JsonSettingsFile, XmlProjectFile};

/// A folder only this test uses (unique name, created exclusively); it goes
/// with the returned value, nothing else.
fn folder(name: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("proava-zwei-{name}-"))
        .tempdir()
        .unwrap()
}

fn launcher(settings: &Path) -> Launcher {
    Launcher::new(
        Box::new(XmlProjectFile::new()),
        Box::new(JsonSettingsFile::at(settings.to_path_buf())),
        Box::new(DiskFolders::new()),
    )
}

fn project_file(folder: &Path, name: &str) -> PathBuf {
    let project = Project::new(
        ProjectName::parse(name).unwrap(),
        "2026-11-17T22:00:00",
        Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
    );
    let file = folder.join(format!("{name}.proava"));
    XmlProjectFile::new().save_project(&file, &project).unwrap();
    file
}

#[test]
fn both_windows_keep_their_recent_projects() {
    let temp = folder("recent");
    let folder = temp.path();
    let settings = folder.join("settings.json");
    let first = launcher(&settings);
    let second = launcher(&settings);
    let a = project_file(folder, "A");
    let b = project_file(folder, "B");
    let c = project_file(folder, "C");
    first.load_project(&a).unwrap();
    second.load_project(&b).unwrap();
    first.load_project(&c).unwrap();
    let names: Vec<String> = launcher(&settings)
        .recent()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(
        names,
        ["C", "B", "A"],
        "keine Liste überschreibt die andere"
    );
    second.forget(&a);
    let names: Vec<String> = launcher(&settings)
        .recent()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["C", "B"], "Entfernen wirkt auch mit zwei Fenstern");
    first.reload_settings();
    assert_eq!(first.recent().len(), 2);
    let leftovers: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "atomar geschrieben, keine Reste");
}

#[test]
fn own_lock_is_set_and_removed() {
    let temp = folder("lock");
    let folder = temp.path();
    let settings = folder.join("settings.json");
    let launcher = launcher(&settings);
    let file = project_file(folder, "Gesperrt");
    assert!(launcher.foreign_lock(&file).is_none());
    launcher.lock_project(&file).unwrap();
    assert!(folder.join(".~lock.Gesperrt.proava#").is_file());
    assert!(
        launcher.foreign_lock(&file).is_none(),
        "die eigene Sperre zählt nicht"
    );
    launcher.unlock_project(&file);
    assert!(!folder.join(".~lock.Gesperrt.proava#").exists());
    // A lock of a program that is gone (same computer) is recognised as stale.
    let host = std::fs::read_to_string("/proc/sys/kernel/hostname").unwrap_or_default();
    if !host.trim().is_empty() {
        std::fs::write(
            folder.join(".~lock.Gesperrt.proava#"),
            format!("proAVA;jemand;{};999999999;17.11.2026 08:00\n", host.trim()),
        )
        .unwrap();
        let lock = launcher.foreign_lock(&file).unwrap();
        assert!(lock.stale, "{lock:?}");
        launcher.remove_stale_lock(&file);
        assert!(launcher.foreign_lock(&file).is_none());
    }
}
