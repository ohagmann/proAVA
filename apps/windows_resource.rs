// Shared by apps/*/build.rs (included with `include!`): Windows resource with
// program symbol and version information, compiled with `llvm-rc` (or `rc.exe`
// on Windows) and handed to the linker as `.res`. Other targets: nothing.
//
// The resource compiler is looked up in this order: PROAVA_RC, RC, `llvm-rc`
// next to the linker of scripts/build_windows.sh (OA_LLVM_BIN), `llvm-rc` /
// `rc` in PATH. Without one the program is built without symbol (warning).

use std::path::{Path, PathBuf};
use std::process::Command;

fn resource_compiler() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for variable in ["PROAVA_RC", "RC"] {
        if let Some(value) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(value));
        }
    }
    if let Some(bin) = std::env::var_os("OA_LLVM_BIN") {
        candidates.push(Path::new(&bin).join("llvm-rc"));
    }
    for name in ["llvm-rc", "rc"] {
        if let Some(path) = std::env::var_os("PATH") {
            for folder in std::env::split_paths(&path) {
                candidates.push(folder.join(name));
                candidates.push(folder.join(format!("{name}.exe")));
            }
        }
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn rc_string(text: &str) -> String {
    text.replace('"', "\"\"")
}

/// `description`: FileDescription, e.g. „proAVA – Ausschreibung, Vergabe, Abrechnung“.
fn windows_resource(internal_name: &str, description: &str, with_icon: bool) {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../windows_resource.rs");
    println!("cargo:rerun-if-env-changed=PROAVA_RC");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let icon = manifest.join("../../assets/icons/proava.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(0);
    let (major, minor, patch) = (
        number("CARGO_PKG_VERSION_MAJOR"),
        number("CARGO_PKG_VERSION_MINOR"),
        number("CARGO_PKG_VERSION_PATCH"),
    );
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let icon_line = if with_icon {
        format!(
            "1 ICON \"{}\"\n",
            rc_string(&icon.canonicalize().unwrap_or(icon).to_string_lossy().replace('\\', "/"))
        )
    } else {
        String::new()
    };
    let rc = format!(
        r#"{icon_line}
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040704B0"
    BEGIN
      VALUE "CompanyName", "Oliver Hagmann"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{internal_name}"
      VALUE "LegalCopyright", "© 2026 Oliver Hagmann – Business Source License 1.1"
      VALUE "OriginalFilename", "{internal_name}.exe"
      VALUE "ProductName", "proAVA"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0407, 1200
  END
END
"#,
        description = rc_string(description),
        internal_name = rc_string(internal_name),
    );
    let rc_file = out.join(format!("{internal_name}.rc"));
    let res_file = out.join(format!("{internal_name}.res"));
    std::fs::write(&rc_file, rc).expect("Ressourcendatei schreiben");
    let Some(compiler) = resource_compiler() else {
        println!("cargo:warning=Kein Ressourcen-Compiler (llvm-rc/rc) gefunden – {internal_name}.exe ohne Symbol und Versionsinfo.");
        return;
    };
    let output = Command::new(&compiler)
        .arg("/nologo")
        // UTF-8 source (umlauts, ©); rc.exe and llvm-rc both know /C.
        .args(["/C", "65001"])
        // No C preprocessor needed (no #include); llvm-rc would otherwise
        // look for clang and take the cross-build flags of cc-rs.
        .arg("/no-preprocess")
        // Relative names: llvm-rc on Unix reads „/home/…“ as an option.
        .current_dir(&out)
        .arg("/FO")
        .arg(format!("{internal_name}.res"))
        .arg(format!("{internal_name}.rc"))
        .output();
    match output {
        Ok(output) if output.status.success() => {
            println!("cargo:rustc-link-arg-bins={}", res_file.display());
        }
        Ok(output) => println!(
            "cargo:warning={} konnte die Ressource nicht übersetzen: {} – {internal_name}.exe ohne Symbol.",
            compiler.display(),
            String::from_utf8_lossy(&output.stderr).trim().replace('\n', " ")
        ),
        Err(error) => println!(
            "cargo:warning={} startet nicht ({error}) – {internal_name}.exe ohne Symbol.",
            compiler.display()
        ),
    }
}
