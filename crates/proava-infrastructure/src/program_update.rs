//! The program file of this process: was it replaced while it runs (an
//! update waiting for a restart)? Rules in
//! `proava_application::program_update`; here the file system is read.

use std::path::Path;
use std::time::SystemTime;

use proava_application::program_update::{replaced, without_deleted_mark};

/// The program file of this process and when the process started.
#[derive(Debug, Clone, Copy)]
pub struct ProgramFile {
    started: SystemTime,
}

impl ProgramFile {
    /// Remembers the start (call once, early).
    pub fn started_now() -> Self {
        Self {
            started: SystemTime::now(),
        }
    }

    /// Whether the program file was replaced since the start (an update is
    /// waiting for a restart). Reads the file system – not every frame.
    pub fn replaced(&self) -> bool {
        let Ok(exe) = std::env::current_exe() else {
            return false;
        };
        let plain = without_deleted_mark(exe.clone());
        let modified = std::fs::metadata(&plain)
            .and_then(|metadata| metadata.modified())
            .ok();
        replaced(&exe, modified, self.started, Path::exists)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_test_program_was_not_replaced() {
        assert!(!ProgramFile::started_now().replaced());
    }
}
