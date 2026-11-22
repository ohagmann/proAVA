use std::fs;
use std::path::{Path, PathBuf};

use proava_application::{FileDetails, Folders, UserFolder};

use crate::paths;

/// The folders of this machine on disk: read, and a new folder on request.
pub struct DiskFolders;

impl Default for DiskFolders {
    fn default() -> Self {
        Self::new()
    }
}

impl DiskFolders {
    pub fn new() -> Self {
        Self
    }
}

impl Folders for DiskFolders {
    fn home(&self) -> PathBuf {
        paths::home_dir()
    }

    fn subfolders(&self, folder: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(folder) else {
            return Vec::new();
        };

        let mut folders: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();

        folders.sort_by_key(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        });
        folders
    }

    fn is_folder(&self, folder: &Path) -> bool {
        folder.is_dir()
    }

    fn files(&self, folder: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(folder) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        files.sort_by_key(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        });
        files
    }

    fn read_file(&self, file: &Path) -> Result<Vec<u8>, String> {
        fs::read(file).map_err(|error| format!("{}: {error}", file.display()))
    }

    fn create_folder(&self, parent: &Path, name: &str) -> Result<PathBuf, String> {
        let folder = parent.join(name);
        if folder.exists() {
            return Err(format!("„{name}“ gibt es hier schon."));
        }
        fs::create_dir(&folder).map_err(|error| match error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                format!("Keine Schreibrechte in {}.", parent.display())
            }
            std::io::ErrorKind::AlreadyExists => format!("„{name}“ gibt es hier schon."),
            _ => format!("Ordner „{name}“ nicht angelegt: {error}"),
        })?;
        Ok(folder)
    }

    fn file_details(&self, file: &Path) -> Option<FileDetails> {
        let metadata = fs::metadata(file)
            .ok()
            .filter(|metadata| metadata.is_file())?;
        let modified_iso = metadata.modified().ok().map(|time| {
            chrono::DateTime::<chrono::Local>::from(time)
                .format("%Y-%m-%dT%H:%M:%S")
                .to_string()
        });
        Some(FileDetails {
            size: metadata.len(),
            modified_iso,
        })
    }

    fn user_folders(&self) -> Vec<(UserFolder, PathBuf)> {
        let Some(dirs) = directories::UserDirs::new() else {
            return Vec::new();
        };
        let home = dirs.home_dir().to_path_buf();
        // Without user-dirs.dirs the usual German and English names count.
        let usual = |names: [&str; 2]| {
            names
                .into_iter()
                .map(|name| home.join(name))
                .find(|folder| folder.is_dir())
        };
        [
            (
                UserFolder::Desktop,
                dirs.desktop_dir(),
                ["Schreibtisch", "Desktop"],
            ),
            (
                UserFolder::Documents,
                dirs.document_dir(),
                ["Dokumente", "Documents"],
            ),
            (
                UserFolder::Pictures,
                dirs.picture_dir(),
                ["Bilder", "Pictures"],
            ),
            (
                UserFolder::Downloads,
                dirs.download_dir(),
                ["Downloads", "Download"],
            ),
        ]
        .into_iter()
        .filter_map(|(kind, folder, names)| {
            folder
                .map(Path::to_path_buf)
                .filter(|folder| *folder != home && folder.is_dir())
                .or_else(|| usual(names))
                .map(|folder| (kind, folder))
        })
        .collect()
    }

    fn drives(&self) -> Vec<PathBuf> {
        #[cfg(windows)]
        {
            // The drive letters from the bit mask of the system: no drive is
            // touched, so a disconnected network drive or an empty card
            // reader does not hold up the dialog. A: and B: are left out.
            // SAFETY: GetLogicalDrives takes no arguments and only returns a mask.
            let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
            drive_letters(mask)
        }
        #[cfg(not(windows))]
        {
            if cfg!(target_os = "macos") {
                let mut drives = vec![PathBuf::from("/")];
                drives.extend(self.subfolders(Path::new("/Volumes")));
                return drives;
            }
            let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
            mounted_disks(&mounts)
                .into_iter()
                .filter(|folder| folder.is_dir())
                .collect()
        }
    }
}

/// `C:\`, `D:\` … for the set bits of a Windows drive mask (bit 0 = A:),
/// without A: and B:.
pub fn drive_letters(mask: u32) -> Vec<PathBuf> {
    (2..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| PathBuf::from(format!("{}:\\", (b'A' + bit) as char)))
        .collect()
}

/// `/` and the mount points of real disks from a Linux `/proc/mounts`
/// (device under `/dev/`), without system mounts like `/boot` or `/snap`.
pub fn mounted_disks(mounts: &str) -> Vec<PathBuf> {
    const SYSTEM: [&str; 11] = [
        "/boot", "/snap", "/var", "/usr", "/proc", "/sys", "/dev", "/run", "/tmp", "/etc", "/efi",
    ];
    let mut disks = vec![PathBuf::from("/")];
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(point)) = (fields.next(), fields.next()) else {
            continue;
        };
        if !device.starts_with("/dev/") {
            continue;
        }
        let point = unescape_mount(point);
        let is_system = SYSTEM
            .iter()
            .any(|system| point == *system || point.starts_with(&format!("{system}/")));
        let removable = point.starts_with("/run/media/");
        if point == "/" || (is_system && !removable) {
            continue;
        }
        let point = PathBuf::from(point);
        if !disks.contains(&point) {
            disks.push(point);
        }
    }
    disks
}

/// `/proc/mounts` writes blanks and some other characters as `\040` (octal).
fn unescape_mount(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let digits = &bytes[index + 1..index + 4];
            if digits.iter().all(|b| (b'0'..=b'7').contains(b)) {
                let value = digits
                    .iter()
                    .fold(0u32, |sum, b| sum * 8 + u32::from(b - b'0'));
                if let Ok(value) = u8::try_from(value) {
                    out.push(value);
                    index += 4;
                    continue;
                }
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod drive_tests {
    use std::path::PathBuf;

    use super::{drive_letters, mounted_disks};

    #[test]
    fn windows_drive_letters_from_the_mask() {
        // A:, C:, D: and Z: set; A: (floppy) is left out.
        let mask = 1 | (1 << 2) | (1 << 3) | (1 << 25);
        assert_eq!(
            drive_letters(mask),
            vec![
                PathBuf::from("C:\\"),
                PathBuf::from("D:\\"),
                PathBuf::from("Z:\\")
            ]
        );
        assert!(drive_letters(0).is_empty());
    }

    #[test]
    fn mounted_data_disks_without_system_mounts() {
        let mounts = "\
sysfs /sys sysfs rw 0 0
/dev/nvme0n1p2 / ext4 rw 0 0
/dev/nvme0n1p1 /boot/efi vfat rw 0 0
/dev/sda1 /daten ext4 rw 0 0
/dev/sdb1 /archiv ext4 rw 0 0
/dev/loop3 /snap/core/1 squashfs ro 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/sdc1 /run/media/nutzer/USB\\040Stick vfat rw 0 0
/dev/sdd1 /media/nutzer/Platte ext4 rw 0 0
/dev/sda1 /daten ext4 rw 0 0
";
        assert_eq!(
            mounted_disks(mounts),
            vec![
                PathBuf::from("/"),
                PathBuf::from("/daten"),
                PathBuf::from("/archiv"),
                PathBuf::from("/run/media/nutzer/USB Stick"),
                PathBuf::from("/media/nutzer/Platte"),
            ]
        );
    }
}
