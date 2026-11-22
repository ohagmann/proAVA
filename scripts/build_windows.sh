#!/usr/bin/env bash
# Baut proava.exe, proava-mcp.exe und proava-web.exe (Browserfenster, WebView2,
# Lader statisch gelinkt; eigener Workspace apps/proava-web) für Windows (x86_64-pc-windows-msvc)
# unter Linux: rustc + lld-link, C-Teile mit clang-cl, gelinkt gegen MSVC-
# Bibliotheken und Windows SDK aus einem vorhandenen Sysroot im
# /winsysroot-Layout (wie von xwin --use-winsysroot-style).
#
#   scripts/build_windows.sh            Release-Build
#   scripts/build_windows.sh --debug    Debug-Build
#   scripts/build_windows.sh --tests    alle Tests für Windows übersetzen (nicht
#                                       ausführen) – prüft #[cfg(windows)]-Code
#   scripts/build_windows.sh --clippy   Clippy für Windows (alle Crates und Tests)
#
# Vorgaben (Umgebungsvariablen):
#   OA_TOOLCHAIN   Grundordner (Vorgabe ~/.local/opt/windows-toolchain)
#   OA_LLVM_BIN    clang-cl, lld-link, llvm-lib (Vorgabe $OA_TOOLCHAIN/llvm20/usr/lib/llvm-20/bin)
#   OA_WINSYSROOT  MSVC + Windows SDK (Vorgabe $OA_TOOLCHAIN/winsysroot)
#   OA_JOBS        parallele Jobs (Vorgabe 8)
#   OA_MEMORY_MAX  Speichergrenze des systemd-Scopes (Vorgabe 24G)
#   OA_TASKS_MAX   Prozess-/Thread-Grenze des Scopes (Vorgabe 2048)
#
# Die MSVC-Bibliotheken und das Windows SDK unterliegen der Lizenz von
# Microsoft. Das Skript lädt nichts herunter und kopiert nichts davon ins Repo.
set -euo pipefail

fehlt() {
    echo "FEHLER: $*" >&2
    exit 1
}

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=x86_64-pc-windows-msvc
PROFIL=release
CARGO_PROFIL=(--release)
TESTS=0
CLIPPY=0
if [ "${1:-}" = "--clippy" ]; then
    CLIPPY=1
    PROFIL=debug
    CARGO_PROFIL=()
elif [ "${1:-}" = "--debug" ]; then
    PROFIL=debug
    CARGO_PROFIL=()
elif [ "${1:-}" = "--tests" ]; then
    TESTS=1
    PROFIL=debug
    CARGO_PROFIL=()
fi

OA_TOOLCHAIN="${OA_TOOLCHAIN:-$HOME/.local/opt/windows-toolchain}"
OA_LLVM_BIN="${OA_LLVM_BIN:-$OA_TOOLCHAIN/llvm20/usr/lib/llvm-20/bin}"
OA_WINSYSROOT="${OA_WINSYSROOT:-$OA_TOOLCHAIN/winsysroot}"
OA_JOBS="${OA_JOBS:-8}"
OA_MEMORY_MAX="${OA_MEMORY_MAX:-24G}"
OA_TASKS_MAX="${OA_TASKS_MAX:-2048}"
export PATH="$HOME/.cargo/bin:$PATH"
# Symbol und Versionsinfo (apps/*/build.rs) übersetzt llvm-rc aus derselben LLVM.
export OA_LLVM_BIN

# --- Prüfungen ---------------------------------------------------------------
command -v cargo >/dev/null 2>&1 || fehlt "cargo nicht gefunden (rustup installieren)."
for tool in clang-cl lld-link llvm-lib; do
    [ -x "$OA_LLVM_BIN/$tool" ] || fehlt "$tool fehlt in $OA_LLVM_BIN (OA_LLVM_BIN setzen)."
done
case "$OA_WINSYSROOT" in
    *" "*) fehlt "Der Pfad OA_WINSYSROOT darf keine Leerzeichen enthalten: $OA_WINSYSROOT" ;;
esac
MSVC_LIB=$(find "$OA_WINSYSROOT/VC/Tools/MSVC" -maxdepth 3 -path '*/lib/x64' -print -quit 2>/dev/null || true)
SDK_LIB=$(find "$OA_WINSYSROOT/Windows Kits/10/Lib" -maxdepth 1 -mindepth 1 -type d -print -quit 2>/dev/null || true)
[ -n "$MSVC_LIB" ] && [ -f "$MSVC_LIB/msvcrt.lib" ] \
    || fehlt "MSVC-Bibliotheken fehlen in $OA_WINSYSROOT/VC/Tools/MSVC/<Version>/lib/x64 (OA_WINSYSROOT setzen)."
[ -n "$SDK_LIB" ] && [ -f "$SDK_LIB/um/x64/kernel32.lib" ] && [ -f "$SDK_LIB/ucrt/x64/ucrt.lib" ] \
    || fehlt "Windows SDK fehlt in $OA_WINSYSROOT/Windows Kits/10/Lib/<Version>/{um,ucrt}/x64."
[ -d "$OA_WINSYSROOT/Windows Kits/10/Include" ] || fehlt "Windows-SDK-Header fehlen in $OA_WINSYSROOT."
rustup target list --installed 2>/dev/null | grep -qx "$TARGET" \
    || fehlt "Rust-Ziel fehlt: rustup target add $TARGET"

echo "proAVA Windows-Build ($PROFIL)"
echo "  LLVM:     $OA_LLVM_BIN ($("$OA_LLVM_BIN/lld-link" --version | head -1))"
echo "  MSVC:     ${MSVC_LIB#$OA_WINSYSROOT/}"
echo "  SDK:      ${SDK_LIB#$OA_WINSYSROOT/}"

# --- Umgebung für Cargo und cc-rs ---------------------------------------------
# Linker: lld-link findet über /winsysroot die neueste MSVC- und SDK-Version
# (lib/x64, um/x64, ucrt/x64). Das umgeht Leerzeichen in "Windows Kits",
# die in RUSTFLAGS nicht gehen.
T=X86_64_PC_WINDOWS_MSVC
t=x86_64_pc_windows_msvc
export CARGO_TARGET_${T}_LINKER="$OA_LLVM_BIN/lld-link"
# /ignore:4099: Microsofts Bibliotheken verweisen auf PDBs, die es hier nicht gibt.
export CARGO_TARGET_${T}_RUSTFLAGS="-C link-arg=/winsysroot:$OA_WINSYSROOT -C target-feature=+crt-static -C link-arg=/ignore:4099"
# C-Teile (z. B. psm/stacker): clang-cl mit denselben Headern
export CC_${t}="$OA_LLVM_BIN/clang-cl"
export CXX_${t}="$OA_LLVM_BIN/clang-cl"
export AR_${t}="$OA_LLVM_BIN/llvm-lib"
CL_FLAGS="--target=$TARGET -Wno-unused-command-line-argument /winsysroot $OA_WINSYSROOT"
export CFLAGS_${t}="$CL_FLAGS"
export CXXFLAGS_${t}="$CL_FLAGS /EHsc"

cd "$ROOT"
# Browserfenster (eigener Workspace) in denselben Zielordner wie proava.exe.
WEB=(--manifest-path apps/proava-web/Cargo.toml --target-dir "${CARGO_TARGET_DIR:-$ROOT/target}")
if [ "$CLIPPY" = 1 ]; then
    systemd-run --user --scope -q -p MemoryMax="$OA_MEMORY_MAX" -p TasksMax="$OA_TASKS_MAX" \
        nice -n 10 cargo clippy --workspace --all-targets --target "$TARGET" -j "$OA_JOBS"
    systemd-run --user --scope -q -p MemoryMax="$OA_MEMORY_MAX" -p TasksMax="$OA_TASKS_MAX" \
        nice -n 10 cargo clippy "${WEB[@]}" --target "$TARGET" -j "$OA_JOBS"
    exit 0
fi
if [ "$TESTS" = 1 ]; then
    systemd-run --user --scope -q -p MemoryMax="$OA_MEMORY_MAX" -p TasksMax="$OA_TASKS_MAX" \
        nice -n 10 cargo test --no-run --workspace --target "$TARGET" -j "$OA_JOBS"
    systemd-run --user --scope -q -p MemoryMax="$OA_MEMORY_MAX" -p TasksMax="$OA_TASKS_MAX" \
        nice -n 10 cargo check "${WEB[@]}" --target "$TARGET" -j "$OA_JOBS"
    echo "Tests für Windows übersetzt (ausführen geht nur unter Windows)."
    exit 0
fi
systemd-run --user --scope -q -p MemoryMax="$OA_MEMORY_MAX" -p TasksMax="$OA_TASKS_MAX" \
    nice -n 10 cargo build "${CARGO_PROFIL[@]}" --target "$TARGET" -j "$OA_JOBS" \
    -p proava -p proava-mcp
systemd-run --user --scope -q -p MemoryMax="$OA_MEMORY_MAX" -p TasksMax="$OA_TASKS_MAX" \
    nice -n 10 cargo build "${CARGO_PROFIL[@]}" --target "$TARGET" -j "$OA_JOBS" "${WEB[@]}"

OUT="${CARGO_TARGET_DIR:-$ROOT/target}/$TARGET/$PROFIL"
echo
for exe in proava.exe proava-mcp.exe proava-web.exe; do
    [ -f "$OUT/$exe" ] || fehlt "$exe wurde nicht erzeugt."
    printf '  %-16s %s\n' "$exe" "$(du -h "$OUT/$exe" | cut -f1)"
done
echo "Fertig: $OUT"
