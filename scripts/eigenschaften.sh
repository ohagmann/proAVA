#!/usr/bin/env bash
# Lange Läufe der Eigenschaftstests (docs/BUILD.md) – mehrere Saaten parallel.
#
#   scripts/eigenschaften.sh                 Saaten 1–4, je 2000 Fälle (Affentest:
#                                            40 Sitzungen), dazu die teuren Tests
#                                            (lang_…) je Saat
#   scripts/eigenschaften.sh 8 5000          Saaten 1–8, je 5000 Fälle
#   FIRST_SEED=9 scripts/eigenschaften.sh 8  Saaten 9–16
#   PROAVA_PROPTEST_STATS=1 scripts/…       mit Statistik in den Logs
#
# Schutz des PCs: gebaut wird in einem systemd-Scope
# (MEMORY_MAX, Standard 16G; TASKS_MAX, Standard 2048), alle Läufe zusammen
# in EINEM Scope mit RUN_MEMORY_MAX (Standard 12G) und RUN_TASKS_MAX
# (Standard 1024), jeder Lauf zusätzlich mit `timeout` (TIMEOUT, Standard
# 600 s). Höchstens PARALLEL Läufe gleichzeitig (Standard 4) mit je
# TEST_THREADS Test-Threads (Standard 2) – so bleibt die Last klein. Ohne
# systemd-run läuft nichts. Logs und Zusammenfassung unter einem Ordner in
# /tmp. Exit-Code 0 nur, wenn alles grün ist.

set -u
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

SEEDS="${1:-4}"
CASES="${2:-2000}"
MEMORY_MAX="${MEMORY_MAX:-16G}"
TASKS_MAX="${TASKS_MAX:-2048}"
RUN_MEMORY_MAX="${RUN_MEMORY_MAX:-12G}"
RUN_TASKS_MAX="${RUN_TASKS_MAX:-1024}"
TIMEOUT="${TIMEOUT:-600}"
PARALLEL="${PARALLEL:-4}"
TEST_THREADS="${TEST_THREADS:-2}"
FIRST_SEED="${FIRST_SEED:-1}"
# Paket, Testdatei, Teiler der Fallzahl (eine Sitzung des Affentests kostet
# etwa so viel wie 50 Domänenfälle).
TESTS=(
    "proava-ui eigenschaften_lv 1"
    "proava-domain eigenschaften_kosten 1"
    "proava-exchange eigenschaften_rundlauf 1"
    "proava-exchange eigenschaften_kaputt 1"
    "proava-infrastructure eigenschaften_einstellungen 1"
    "proava-ui affentest 50"
)

if ! { command -v systemd-run >/dev/null 2>&1 && systemd-run --user --scope -q true 2>/dev/null; }; then
    echo "systemd-run fehlt – ohne Speicher- und Prozessgrenze laufen die Tests nicht."
    exit 1
fi

# ----- Zweiter Teil: die Läufe (im begrenzten Scope, siehe unten) -----
if [ "${1:-}" = "--laeufe" ]; then
    LOG_DIR="$2"
    shift 2
    running=0
    for job in "$@"; do
        IFS='|' read -r name seed cases binary filter <<<"$job"
        (
            # The expensive runs (lang_…) keep their own number of cases.
            [ -n "$cases" ] && export PROAVA_PROPTEST_CASES="$cases"
            # shellcheck disable=SC2086
            PROAVA_PROPTEST_SEED="$seed" \
                timeout "$TIMEOUT" nice -n 15 "$binary" $filter --test-threads "$TEST_THREADS" \
                >"$LOG_DIR/$name.log" 2>&1
            echo "exit $?" >>"$LOG_DIR/$name.log"
        ) &
        running=$((running + 1))
        if [ "$running" -ge "$PARALLEL" ]; then
            wait -n
            running=$((running - 1))
        fi
    done
    wait
    exit 0
fi

LOG_DIR="$(mktemp -d "${TMPDIR:-/tmp}/proava-eigenschaften.XXXXXX")"
echo "Eigenschaftstests: $SEEDS Saaten, je $CASES Fälle – Logs: $LOG_DIR"
JOBS=()
for entry in "${TESTS[@]}"; do
    read -r package test divisor <<<"$entry"
    binary=$(systemd-run --user --scope -q -p MemoryMax="$MEMORY_MAX" -p TasksMax="$TASKS_MAX" \
        nice -n 10 cargo test -p "$package" --test "$test" --no-run --message-format=json -j 6 2>/dev/null |
        sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | tail -n 1)
    if [ -z "$binary" ]; then
        echo "Bauen fehlgeschlagen: $package/$test"
        exit 1
    fi
    cases=$((CASES / divisor))
    [ "$cases" -lt 5 ] && cases=5
    for seed in $(seq "$FIRST_SEED" $((FIRST_SEED + SEEDS - 1))); do
        JOBS+=("$test-$seed|$seed|$cases|$binary|")
        JOBS+=("$test-lang-$seed|$seed||$binary|--ignored lang_")
    done
done

# All runs together in one limited scope.
systemd-run --user --scope -q -p MemoryMax="$RUN_MEMORY_MAX" -p TasksMax="$RUN_TASKS_MAX" \
    "$0" --laeufe "$LOG_DIR" "${JOBS[@]}"

FAILED=0
for log in "$LOG_DIR"/*.log; do
    if grep -q '^exit 0$' "$log"; then
        printf '  grün  %s\n' "$(basename "$log" .log)"
    else
        FAILED=1
        printf '  ROT   %s\n' "$(basename "$log" .log)"
        grep -a -A 3 -E '^Test failed|panicked at|^exit 124' "$log" | head -n 8 | sed 's/^/        /'
    fi
done
echo
if [ "$FAILED" = 0 ]; then
    echo "EIGENSCHAFTEN: grün"
else
    echo "EIGENSCHAFTEN: ROT (Logs: $LOG_DIR)"
fi
exit "$FAILED"
