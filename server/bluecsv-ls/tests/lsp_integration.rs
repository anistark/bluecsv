//! End-to-end test: spawn the `bluecsv-ls` binary, speak LSP over stdio,
//! and check the responses to `hover`, `completion`, and
//! `workspace/executeCommand` requests. This is the substitute for an
//! in-Zed E2E test — Zed doesn't offer a scriptable harness, but the
//! language server surface is standard LSP.
//!
//! The test is serialized: one long-lived `bluecsv-ls` child process
//! handles the full request/response sequence. Cargo sets
//! `CARGO_BIN_EXE_bluecsv-ls` for integration tests of a binary crate, so
//! no path wiring is needed.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

/// Minimal LSP-over-stdio client. Sends a JSON-RPC request, reads framed
/// replies, and dispatches responses vs. notifications by presence of an
/// `id` field.
struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Client {
    fn spawn() -> Self {
        let exe = env!("CARGO_BIN_EXE_bluecsv-ls");
        let mut child = Command::new(exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn bluecsv-ls");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn send(&mut self, msg: &Value) {
        let body = serde_json::to_string(msg).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).unwrap();
        self.stdin.write_all(body.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Send a request and drain messages until the matching response arrives.
    /// Notifications published by the server (e.g. diagnostics) are ignored.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        loop {
            let msg = self.read_message();
            if msg.get("id").and_then(Value::as_i64) == Some(id) {
                return msg;
            }
            // otherwise: notification or a response to a prior request — skip
        }
    }

    fn read_message(&mut self) -> Value {
        // Parse LSP framing: one or more headers ending with `\r\n\r\n`, then
        // exactly Content-Length bytes of JSON body.
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read header");
            if n == 0 {
                panic!("server closed stdout");
            }
            if line == "\r\n" {
                break;
            }
            if let Some(rest) = line.strip_prefix("Content-Length:") {
                content_length = Some(rest.trim().parse().expect("parse Content-Length"));
            }
        }
        let len = content_length.expect("missing Content-Length");
        let mut body = vec![0u8; len];
        self.stdout.read_exact(&mut body).expect("read body");
        serde_json::from_slice(&body).expect("parse json")
    }

    fn shutdown(mut self) {
        let _ = self.request("shutdown", json!(null));
        self.notify("exit", json!(null));
        // Give the server a moment, then force-kill so a hung child doesn't
        // wedge the test runner.
        for _ in 0..20 {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
    }
}

fn init_params(root_uri: &str) -> Value {
    json!({
        "processId": std::process::id(),
        "rootUri": root_uri,
        "capabilities": {
            "textDocument": {
                "hover": {"contentFormat": ["markdown", "plaintext"]},
                "completion": {},
                "publishDiagnostics": {}
            },
            "workspace": {"executeCommand": {}}
        }
    })
}

const DOC_URI: &str = "file:///tmp/bluecsv-lsp-test.csv";
const DOC: &str = "id,name,count\n1,Alice,10\n22,Bob,42\n333,Carol,7\n";

#[test]
fn lsp_hover_completion_column_summary() {
    let mut c = Client::spawn();

    // 1. initialize + initialized
    let init = c.request("initialize", init_params("file:///tmp"));
    assert!(init.get("result").is_some(), "initialize failed: {init}");
    c.notify("initialized", json!({}));

    // 2. didOpen
    c.notify(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": DOC_URI,
                "languageId": "csv",
                "version": 1,
                "text": DOC,
            }
        }),
    );

    // 3. Hover on the "count" header cell (row 0, inside "count").
    //    Header is at line 0, "count" starts at col 8 (id,name,count).
    let hover = c.request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": DOC_URI},
            "position": {"line": 0, "character": 10},
        }),
    );
    let hover_md = hover
        .pointer("/result/contents/value")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("hover shape unexpected: {hover}"));
    assert!(
        hover_md.contains("count"),
        "hover should mention column name: {hover_md}"
    );
    assert!(
        hover_md.contains("header"),
        "header-cell hover should say 'header': {hover_md}"
    );

    // 4. Hover on a data cell in an int column — should surface `type: int`.
    //    Line 1 is `1,Alice,10`, column 'count' starts at char 8.
    let hover_data = c.request(
        "textDocument/hover",
        json!({
            "textDocument": {"uri": DOC_URI},
            "position": {"line": 1, "character": 9},
        }),
    );
    let md = hover_data
        .pointer("/result/contents/value")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("hover shape unexpected: {hover_data}"));
    assert!(
        md.contains("type: int"),
        "data-cell hover should include inferred type: {md}"
    );

    // 5. Completion inside the `name` column on a fresh row — values
    //    "Alice", "Bob", "Carol" have been seen.
    let completion = c.request(
        "textDocument/completion",
        json!({
            "textDocument": {"uri": DOC_URI},
            "position": {"line": 2, "character": 3},
        }),
    );
    let items = completion
        .pointer("/result")
        .cloned()
        .unwrap_or(Value::Null);
    let items_array: Vec<String> = match &items {
        Value::Array(a) => a,
        Value::Object(o) => o.get("items").and_then(Value::as_array).expect("items"),
        _ => panic!("completion result not array/object: {items}"),
    }
    .iter()
    .filter_map(|v| v.get("label").and_then(Value::as_str).map(String::from))
    .collect();
    assert!(
        items_array.iter().any(|s| s == "Alice"),
        "expected 'Alice' in completion labels: {items_array:?}"
    );

    // 6. executeCommand: bluecsv.columnSummary for column 2 (count).
    let exec = c.request(
        "workspace/executeCommand",
        json!({
            "command": "bluecsv.columnSummary",
            "arguments": [{"uri": DOC_URI, "col": 2}],
        }),
    );
    let result = exec
        .pointer("/result")
        .unwrap_or_else(|| panic!("columnSummary missing result: {exec}"));
    assert_eq!(
        result.get("type").and_then(Value::as_str),
        Some("int"),
        "count column should be inferred as int: {result}"
    );
    assert_eq!(
        result.get("count").and_then(Value::as_u64),
        Some(3),
        "count should see 3 non-empty values: {result}"
    );
    assert_eq!(
        result.get("sum").and_then(Value::as_f64),
        Some(59.0),
        "sum of 10+42+7 should be 59: {result}"
    );

    c.shutdown();
}
