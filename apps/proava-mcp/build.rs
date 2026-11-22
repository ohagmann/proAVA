//! Windows: program symbol and version information (see ../windows_resource.rs).
include!("../windows_resource.rs");

fn main() {
    windows_resource("proava-mcp", "proAVA MCP-Server", true);
}
