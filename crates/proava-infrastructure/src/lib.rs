//! Adapters that talk to the outside world: project files (zip container with
//! XML and images), zip backups,
//! settings in JSON, the folders of this machine, the clock and the published
//! catalogue lists of AUSSCHREIBEN.DE (via curl, cached).

pub mod disk_folders;
pub mod json_settings;
pub mod paths;
pub mod program_update;
mod project_billing;
mod project_costs;
mod project_library;
pub mod project_lock;
mod project_price_check;
mod project_stlb;
mod project_structure;
mod project_tracking;
pub mod project_xml;
pub mod web_texts;
pub mod window_bridge;
pub mod xml_project;
mod xml_tree;

pub use disk_folders::DiskFolders;
pub use json_settings::JsonSettingsFile;
pub use web_texts::CurlWebTexts;
pub use xml_project::{project_from_bytes, project_to_bytes, XmlProjectFile};
