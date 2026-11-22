//! Tastenkürzel-Übersicht als CSV (Bereich;Befehl;Tasten) für das Handbuch –
//! aus denselben Definitionen wie die Hilfe im Programm. Normalerweise über
//! `scripts/handbuch.sh`.
//!
//! ```sh
//! cargo run -p proava-ui --example tastenkuerzel > docs/handbuch/tastenkuerzel.csv
//! ```

fn main() {
    print!("{}", proava_ui::shortcuts::csv());
}
