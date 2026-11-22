//! Automatic recovery copy on a worker thread: the frame only takes the
//! snapshot of the project (cheap, long texts are shared); dropping unused
//! images, serializing and writing run outside, so the window does not
//! stall. Saving and discarding wait for a running copy first – no stale
//! recovery file is written after them.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use proava_application::ProjectRepository;
use proava_domain::Project;

#[derive(Default)]
pub struct BackgroundAutosave {
    running: Option<JoinHandle<Result<PathBuf, String>>>,
}

impl BackgroundAutosave {
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Writes `project` (a snapshot) to `file` with `storage`.
    pub fn start(
        &mut self,
        storage: Arc<dyn ProjectRepository + Send + Sync>,
        file: PathBuf,
        mut project: Project,
    ) {
        self.running = Some(thread::spawn(move || {
            project.remove_unused_images();
            storage.save_project(&file, &project).map(|()| file)
        }));
    }

    /// The result once the copy is written; `None` while it runs or when
    /// nothing was started.
    pub fn finished(&mut self) -> Option<Result<PathBuf, String>> {
        if self.running.as_ref().is_some_and(JoinHandle::is_finished) {
            self.wait()
        } else {
            None
        }
    }

    /// Waits for a running copy (before saving or discarding).
    pub fn wait(&mut self) -> Option<Result<PathBuf, String>> {
        let handle = self.running.take()?;
        Some(
            handle
                .join()
                .unwrap_or_else(|_| Err("Abbruch im Hintergrund (interner Fehler).".to_string())),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Mutex;

    use proava_domain::{Lv, OzMask, ProjectName};

    use super::*;

    /// Storage that records what it wrote, or fails.
    struct Recorder {
        written: Mutex<Vec<PathBuf>>,
        fail: bool,
    }

    impl ProjectRepository for Recorder {
        fn create_project(&self, _: &Path, _: &ProjectName) -> Result<PathBuf, String> {
            unreachable!()
        }
        fn load_project(&self, _: &Path) -> Result<Project, String> {
            unreachable!()
        }
        fn save_project(&self, file: &Path, _: &Project) -> Result<(), String> {
            if self.fail {
                return Err("Datenträger voll".into());
            }
            self.written.lock().unwrap().push(file.to_path_buf());
            Ok(())
        }
        fn backup(&self, _: &Path, _: &Path) -> Result<PathBuf, String> {
            unreachable!()
        }
        fn restore(&self, _: &Path, _: &Path) -> Result<PathBuf, String> {
            unreachable!()
        }
        fn modified_iso(&self, _: &Path) -> Option<String> {
            None
        }
        fn remove_file(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
    }

    fn project() -> Project {
        Project::new(
            ProjectName::parse("Test").unwrap(),
            "2026-11-17T18:00:00",
            Lv::new("LV", OzMask::parse(OzMask::DEFAULT_PATTERN).unwrap()),
        )
    }

    #[test]
    fn writes_in_the_background_and_reports_the_result() {
        let storage = Arc::new(Recorder {
            written: Mutex::new(Vec::new()),
            fail: false,
        });
        let mut autosave = BackgroundAutosave::default();
        assert!(autosave.wait().is_none());
        autosave.start(
            storage.clone(),
            PathBuf::from("/x/.a.proava.autosave"),
            project(),
        );
        assert!(autosave.is_running());
        assert_eq!(
            autosave.wait(),
            Some(Ok(PathBuf::from("/x/.a.proava.autosave")))
        );
        assert!(!autosave.is_running());
        assert_eq!(storage.written.lock().unwrap().len(), 1);

        let failing = Arc::new(Recorder {
            written: Mutex::new(Vec::new()),
            fail: true,
        });
        autosave.start(failing, PathBuf::from("/x/b"), project());
        let result = loop {
            if let Some(result) = autosave.finished() {
                break result;
            }
            std::thread::yield_now();
        };
        assert_eq!(result, Err("Datenträger voll".to_string()));
        assert!(autosave.finished().is_none());
    }
}
