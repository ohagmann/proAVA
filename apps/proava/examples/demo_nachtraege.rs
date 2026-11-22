//! Writes the synthetic demo project with contract and six invented
//! Nachträge (`proava_ui::demo_change_orders`) as a `.proava` file – for
//! screenshots of the Handbuch and for trying the Nachträge.
//!
//! ```sh
//! cargo run -p proava --example demo_nachtraege -- /tmp/Demo/Nachtraege.proava
//! ```

use std::path::PathBuf;

use proava_application::ProjectRepository;
use proava_infrastructure::XmlProjectFile;

fn main() -> Result<(), String> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Aufruf: demo_nachtraege <Datei.proava>")?;
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let project = proava_ui::demo_change_orders::change_order_demo_project();
    XmlProjectFile::new().save_project(&path, &project)?;
    println!("geschrieben: {}", path.display());
    Ok(())
}
