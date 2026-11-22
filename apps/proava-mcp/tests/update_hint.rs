//! The MCP server runs on after an update replaced its program file (old
//! code): the next tool answer says so – once per session. The program file
//! is not read here; a switch stands in for it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use proava_exchange::GaebPdfExchange;
use proava_infrastructure::XmlProjectFile;
use proava_mcp::{Server, UPDATE_HINT};
use serde_json::json;

/// The text of a `tools/call` answer (an unknown project: a quick error).
fn answer(server: &mut Server, id: u64) -> String {
    let reply = server
        .handle(&json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": "projekt_lesen", "arguments": {"projekt": "/gibt/es/nicht.proava"}}}))
        .expect("Antwort");
    reply["result"]["content"][0]["text"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| panic!("kein Text: {reply}"))
}

fn server_with(updated: Arc<AtomicBool>) -> Server {
    Server::new(Box::new(XmlProjectFile::new()), Box::new(GaebPdfExchange))
        .with_update_check(Box::new(move || updated.load(Ordering::SeqCst)))
}

#[test]
fn an_update_is_said_once_in_the_next_answer() {
    let updated = Arc::new(AtomicBool::new(false));
    let mut server = server_with(updated.clone());
    assert!(
        !answer(&mut server, 1).contains(UPDATE_HINT),
        "noch kein Update"
    );

    updated.store(true, Ordering::SeqCst);
    let first = answer(&mut server, 2);
    assert!(
        first.ends_with(&format!("\n\n{UPDATE_HINT}")),
        "Hinweis als eigene Zeile am Ende: {first}"
    );
    assert!(
        first.starts_with(&answer_without_hint()),
        "die eigentliche Antwort bleibt: {first}"
    );
    assert!(
        !answer(&mut server, 3).contains(UPDATE_HINT),
        "nur einmal je Sitzung"
    );
    // Other methods carry no hint.
    let list = server
        .handle(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list"}))
        .unwrap();
    assert!(!list.to_string().contains(UPDATE_HINT));
}

#[test]
fn without_a_check_there_is_no_hint() {
    let mut server = Server::new(Box::new(XmlProjectFile::new()), Box::new(GaebPdfExchange));
    assert!(!answer(&mut server, 1).contains(UPDATE_HINT));
}

/// The same answer from a server that knows of no update.
fn answer_without_hint() -> String {
    answer(&mut server_with(Arc::new(AtomicBool::new(false))), 9)
}
