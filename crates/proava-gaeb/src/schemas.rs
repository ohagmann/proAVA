//! Where the official GAEB XSDs lie on this machine – for the Prüfstand and
//! tests that validate written files (the XSDs are not part of the
//! repository, see `docs/BUILD.md`; `scripts/gaeb_muster_laden.sh`
//! loads them).
//!
//! Searched in this order, each folder with all its subfolders:
//! `$PROAVA_GAEB_XSD_DIR`, `$PROAVA_GAEB_DATA/xsd`,
//! `${XDG_DATA_HOME:-~/.local/share}/proava/gaeb/xsd`.

use std::path::{Path, PathBuf};

/// Folders that are searched, in order (existing or not).
pub fn schema_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    if let Some(dir) = std::env::var_os("PROAVA_GAEB_XSD_DIR").filter(|dir| !dir.is_empty()) {
        folders.push(PathBuf::from(dir));
    }
    if let Some(data) = std::env::var_os("PROAVA_GAEB_DATA").filter(|dir| !dir.is_empty()) {
        folders.push(PathBuf::from(data).join("xsd"));
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".local").join("share"))
        });
    if let Some(data_home) = data_home {
        folders.push(data_home.join("proava").join("gaeb").join("xsd"));
    }
    folders
}

/// The first folder that holds the GAEB DA XML 3.3 schemas (somewhere below
/// it), None when none is found.
pub fn schema_folder() -> Option<PathBuf> {
    schema_folders()
        .into_iter()
        .find(|folder| contains_schema(folder, 0))
}

fn contains_schema(folder: &Path, depth: usize) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 6 && contains_schema(&path, depth + 1) {
                return true;
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("GAEB_DA_XML_8") && name.contains("_3.3_"))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_schemas_in_subfolders() {
        // A folder only this test uses (unique, created exclusively), removed
        // with `scratch` at the end of the test – nothing else.
        let scratch = tempfile::Builder::new()
            .prefix("proava-xsd-")
            .tempdir()
            .unwrap();
        let root = scratch.path().to_path_buf();
        let deep = root.join("2021-05_Leistungsverzeichnis").join("XSD");
        std::fs::create_dir_all(&deep).unwrap();
        assert!(!contains_schema(&root, 0));
        std::fs::write(deep.join("GAEB_DA_XML_83_3.3_2021-05.xsd"), b"<x/>").unwrap();
        assert!(contains_schema(&root, 0));
        std::fs::remove_dir_all(&root).ok();
    }
}
