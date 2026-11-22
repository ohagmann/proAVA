//! MCP server for proAVA: lets AI assistants read and write bills of
//! quantities in `.proava` project files over stdio (JSON-RPC 2.0).

mod args;
mod ausschreiben_tools;
mod bidder_tools;
mod billing_tools;
mod browser_window;
mod catalog;
mod change_order_tools;
mod compare_tools;
mod correspondence_tools;
mod cost_lv_tools;
mod cost_tools;
mod exchange_tools;
mod format;
mod frame_args;
mod library_tools;
mod lv_tools;
mod markup;
mod read_tools;
mod search_tools;
mod server;
mod stlb_tools;
mod template_tools;
mod tracking_tools;
mod transfer_tools;
mod window_tools;
mod write_tools;

use std::path::PathBuf;

pub use browser_window::{BrowserWindow, HelperWindow, NO_PROGRAMS};
pub use server::{
    real_path, run_stdio, LibraryDefaults, PrintDefaults, Server, SpellDefaults, TemplateDefaults,
    PROTOCOL_VERSION, SERVER_VERSION, UPDATE_HINT,
};
pub use window_tools::{BridgeWindows, WindowLink};

/// Environment variable with the folders the server may touch, separated by
/// `:` (Windows: `;`, as in `PATH`).
pub const ROOTS_VARIABLE: &str = "PROAVA_MCP_ORDNER";

/// Allowed folders from [`ROOTS_VARIABLE`]; empty = no restriction.
pub fn allowed_roots_from_env() -> Vec<PathBuf> {
    std::env::var_os(ROOTS_VARIABLE)
        .map(|value| {
            std::env::split_paths(&value)
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default()
}
