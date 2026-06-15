//! `allem-mcp` — a minimal MCP server over stdio (newline-delimited JSON-RPC 2.0).
//!
//! Implements just enough of the protocol to be driven by editors/agents: `initialize`,
//! `tools/list`, `tools/call`, and `ping`; notifications are accepted and ignored. The tools
//! expose Allem's deterministic engine as structured truth (see [`tools`]).

pub mod tools;

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the server loop on stdin/stdout until EOF. Returns on clean shutdown.
pub fn serve_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(&req),
            Err(_) => Some(error(Value::Null, -32700, "parse error")),
        };
        if let Some(resp) = response {
            serde_json::to_writer(&mut out, &resp)?;
            out.write_all(b"\n")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Dispatch a single JSON-RPC message. Returns `None` for notifications (no `id`).
pub fn handle(req: &Value) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    // Notifications have no `id` and never get a response.
    let id = req.get("id").cloned()?;

    let response = match method {
        "initialize" => success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "allem", "version": env!("CARGO_PKG_VERSION") }
            }),
        ),
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({ "tools": tools::definitions() })),
        "tools/call" => tools_call(id, req),
        other => error(id, -32601, &format!("method not found: {other}")),
    };
    Some(response)
}

fn tools_call(id: Value, req: &Value) -> Value {
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    // Run the tool inside catch_unwind so a panic in analysis returns an error result instead of
    // killing the whole server (one bad repo/file must not take down the session).
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tools::call(name, &args)))
            .unwrap_or_else(|_| Err(format!("internal error: tool '{name}' panicked")));

    match outcome {
        Ok(payload) => {
            let text = serde_json::to_string_pretty(&payload)
                .unwrap_or_else(|_| "<unserializable>".to_string());
            success(
                id,
                json!({
                    "content": [{ "type": "text", "text": text }],
                    "isError": false
                }),
            )
        }
        // Tool-level failures are returned as an error tool result, not a transport error,
        // so the agent can read the message and adjust.
        Err(message) => success(
            id,
            json!({
                "content": [{ "type": "text", "text": message }],
                "isError": true
            }),
        ),
    }
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn initialize_advertises_protocol_and_tools() {
        let req = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" });
        let resp = handle(&req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notifications_get_no_response() {
        let req = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(handle(&req).is_none());
    }

    #[test]
    fn tools_list_returns_tools() {
        let req = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
        let resp = handle(&req).unwrap();
        assert!(resp["result"]["tools"].is_array());
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let req = json!({ "jsonrpc": "2.0", "id": 3, "method": "bogus" });
        let resp = handle(&req).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }
}
