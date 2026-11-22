#!/usr/bin/env bash
# Abnahmeprüfung nach jedem Merge – ein Aufruf, eine Zusammenfassung.
#
#   scripts/check.sh            Format (nur Hinweis), Build, alle Tests
#                               (inkl. UI-Tests ohne Bildschirm), Clippy
#   PROAVA_GAEB_TESTDATA=/ordner/a:/ordner/b scripts/check.sh
#                               zusätzlich Prüfstand GAEB: alle echten
#                               GAEB-XML-Dateien lesen und zurückschreiben
#
# Umgebung: JOBS (Standard 4), CPU_QUOTA (Standard 800% = 8 Kerne), MEMORY_MAX (Standard 24G), TASKS_MAX (Standard
# 2048 Prozesse/Threads; Builds und Tests laufen in einem systemd-Scope mit
# diesen Grenzen, wenn systemd-run da ist). TASKS_MAX bremst einen Test, der
# sich selbst immer wieder startet (Nacht 17./18.11.2026), bevor er den
# Speicher aller Agenten aufbraucht.
# Exit-Code 0 nur, wenn Build, Tests (und ggf. Prüfstand) grün sind;
# Format und Clippy sind informativ.

set -u
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

JOBS="${JOBS:-4}"
CPU_QUOTA="${CPU_QUOTA:-800%}"
MEMORY_MAX="${MEMORY_MAX:-24G}"
TASKS_MAX="${TASKS_MAX:-2048}"
LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/proava-check.XXXXXX")"

run_limited() {
    if command -v systemd-run >/dev/null 2>&1 && systemd-run --user --scope -q true 2>/dev/null; then
        systemd-run --user --scope -q -p MemoryMax="$MEMORY_MAX" -p TasksMax="$TASKS_MAX" -p CPUQuota="$CPU_QUOTA" nice -n 19 "$@"
    else
        nice -n 10 "$@"
    fi
}

NAMES=()
RESULTS=()
FAILED=0

# step <name> <pflicht|info> <befehl …> – Ausgabe ins Log, Ergebnis merken.
step() {
    local name="$1" kind="$2"
    shift 2
    local log="$LOG_DIR/${#NAMES[@]}.log"
    printf '▸ %s … ' "$name"
    local start=$SECONDS
    if "$@" >"$log" 2>&1; then
        RESULTS+=("ok")
        printf 'ok (%ss)\n' "$((SECONDS - start))"
    else
        if [ "$kind" = pflicht ]; then
            RESULTS+=("FEHLER")
            FAILED=1
        else
            RESULTS+=("Hinweis")
        fi
        printf '%s (%ss) – Log: %s\n' "${RESULTS[-1]}" "$((SECONDS - start))" "$log"
        [ "$kind" = pflicht ] && tail -n 25 "$log" | sed 's/^/    /'
    fi
    NAMES+=("$name")
}

echo "proAVA Abnahmeprüfung · $(git rev-parse --abbrev-ref HEAD) @ $(git rev-parse --short HEAD)"
echo

# Das Browserfenster proava-web ist ein eigener Workspace (wry, Browser-Engine des Systems);
# es landet im selben Zielordner wie proava (dort sucht proAVA es).
WEB=(--manifest-path apps/proava-web/Cargo.toml --target-dir "${CARGO_TARGET_DIR:-target}")
# proava und proava-mcp dürfen keine Browser-Engine im Abhängigkeitsbaum haben.
ohne_browser_engine() {
    ! cargo tree -p proava -p proava-mcp -e normal,build --prefix none \
        | grep -E '^(wry|tao|webkit2gtk|javascriptcore-rs|soup3|webview2-com)(-sys)? '
}

# Im Repo steht nur Doku für Nutzer und Entwickler, keine Arbeitsnotizen: in docs/ nur die erlaubten
# Dateien, keine Verweise auf andere Dateien unter docs/, keine Zweignamen.
DOKU_ERLAUBT='docs/(BUILD\.md|MCP\.md|FACHLICHES\.md|handbuch/.*)'
nur_erlaubte_doku() {
    local fehler=0 funde
    funde=$(git ls-files -- docs | grep -v -x -E "$DOKU_ERLAUBT")
    if [ -n "$funde" ]; then
        printf 'Nicht erlaubte Dateien unter docs/:\n%s\n' "$funde"
        fehler=1
    fi
    funde=$(git grep -n -I -o -E 'docs/[A-Za-z0-9_./-]*[A-Za-z0-9_/-]' -- . \
        | awk -v erlaubt="^$DOKU_ERLAUBT\$" '{ m = $0; sub(/^[^:]*:[0-9]+:/, "", m); if (m !~ erlaubt) print }')
    if [ -n "$funde" ]; then
        printf 'Verweise auf Dateien unter docs/, die es nicht gibt bzw. nicht geben soll:\n%s\n' "$funde"
        fehler=1
    fi
    funde=$(git grep -n -I -E 'fe[a]t/[a-z]' -- .)
    if [ -n "$funde" ]; then
        printf 'Zweignamen in getrackten Dateien:\n%s\n' "$funde"
        fehler=1
    fi
    # Arbeitsnotizen: datierte Zitate, Pfade der Entwicklungsrechner, interne Regelwerke.
    funde=$(git grep -n -I -E 'Olive[r] [0-9]{1,2}\.[0-9]{1,2}\.|/tmp/opencod[e]|/VM/Repo[s]|/Data[2]|opencirt-toolchai[n]|[Tt]eamrege[l]|team rul[e]' -- .)
    if [ -n "$funde" ]; then
        printf 'Arbeitsnotizen in getrackten Dateien (datierte Zitate, lokale Pfade, interne Regeln):\n%s\n' "$funde"
        fehler=1
    fi
    return "$fehler"
}

step "Doku (nur erlaubte docs/, keine Zweignamen, keine Arbeitsnotizen)" pflicht nur_erlaubte_doku
step "Format (cargo fmt --check)" info cargo fmt --all -- --check --color never
step "Build (cargo build --workspace)" pflicht run_limited cargo build --workspace -j "$JOBS"
step "Tests (cargo test --workspace)" pflicht run_limited cargo test --workspace -j "$JOBS"
step "Clippy" info run_limited cargo clippy --workspace --all-targets -j "$JOBS"
step "Ohne Browser-Engine (cargo tree proava, proava-mcp)" pflicht ohne_browser_engine
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
    step "Browserfenster proava-web (Build)" pflicht run_limited cargo build "${WEB[@]}" -j "$JOBS"
    step "Browserfenster proava-web (Format)" info \
        cargo fmt --manifest-path apps/proava-web/Cargo.toml -- --check --color never
    step "Browserfenster proava-web (Clippy)" info run_limited cargo clippy "${WEB[@]}" -j "$JOBS"
else
    echo "▸ Browserfenster proava-web … übersprungen (WebKitGTK 4.1 fehlt: libwebkit2gtk-4.1-dev)"
fi

if [ -n "${PROAVA_GAEB_TESTDATA:-}" ]; then
    step "Prüfstand GAEB (echte Dateien)" pflicht run_limited cargo test -p proava-gaeb --test xml_codec \
        -j "$JOBS" -- --ignored real_files_read_and_roundtrip --nocapture
fi

echo
echo "Zusammenfassung"
for i in "${!NAMES[@]}"; do
    printf '  %-8s %s\n' "${RESULTS[$i]}" "${NAMES[$i]}"
done

# Testzahlen aus dem Test-Log.
for i in "${!NAMES[@]}"; do
    case "${NAMES[$i]}" in
    Tests*)
        awk '/^test result:/ { for (f = 1; f <= NF; f++) { if ($(f+1) ~ /^passed;?$/) p += $f; if ($(f+1) ~ /^failed;?$/) x += $f; if ($(f+1) ~ /^ignored;?$/) g += $f } }
             END { printf "  Tests:   %d grün, %d rot, %d ignoriert\n", p, x, g }' "$LOG_DIR/$i.log"
        grep -E '^test .* FAILED$' "$LOG_DIR/$i.log" | sed 's/^/    /'
        ;;
    Clippy)
        warnings=$(grep -E '^warning' "$LOG_DIR/$i.log" | grep -cv 'generated [0-9]* warning')
        errors=$(grep -cE '^error' "$LOG_DIR/$i.log")
        printf '  Clippy:  %d Warnungen, %d Fehler' "$warnings" "$errors"
        if [ "$warnings" -gt 0 ]; then
            printf ' – häufigste:\n'
            grep -E '^warning: ' "$LOG_DIR/$i.log" | grep -v 'generated [0-9]* warning' |
                sort | uniq -c | sort -rn | head -n 8 | sed 's/^/    /'
        else
            printf '\n'
        fi
        ;;
    Format*)
        [ "${RESULTS[$i]}" = ok ] || printf '  Format:  %d Dateien weichen von rustfmt ab (nur Hinweis)\n' \
            "$(sed -n 's/^Diff in \(.*\):[0-9]*:$/\1/p' "$LOG_DIR/$i.log" | sort -u | wc -l)"
        ;;
    esac
done
[ -n "${PROAVA_GAEB_TESTDATA:-}" ] || echo "  (Prüfstand GAEB übersprungen: PROAVA_GAEB_TESTDATA nicht gesetzt)"
echo "  Logs: $LOG_DIR"
echo
if [ "$FAILED" = 0 ]; then
    echo "ABNAHME: grün"
else
    echo "ABNAHME: ROT"
fi
exit "$FAILED"
