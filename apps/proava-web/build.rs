//! Windows: program symbol and version information (see ../windows_resource.rs).
include!("../windows_resource.rs");

fn main() {
    windows_resource(
        "proava-web",
        "proAVA – Browserfenster für AUSSCHREIBEN.DE und STLB-Bau Online",
        true,
    );
}
