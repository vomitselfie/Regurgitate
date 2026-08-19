use std::{
    io::{BufRead, Write},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::application::HealthReport;

use super::view;

const HOST_RPC_TIMEOUT: Duration = Duration::from_secs(10);
const FIRST_OUTBOUND_ID: u64 = 1_000_000;
const STATUS_COMMAND: &str = "plugin.vomitselfie.praxis.status";
const REFRESH_COMMAND: &str = "plugin.vomitselfie.praxis.refresh";

enum Incoming {
    Message(Value),
    End,
}

pub(super) fn run<R, W, F>(reader: R, writer: W, inspect: F) -> Result<()>
where
    R: BufRead + Send + 'static,
    W: Write,
    F: Fn() -> HealthReport,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let Ok(message) = serde_json::from_str::<Value>(line.trim()) else {
                continue;
            };
            if sender.send(Incoming::Message(message)).is_err() {
                return;
            }
        }
        let _ = sender.send(Incoming::End);
    });

    Worker::new(writer, receiver).serve(inspect)
}

struct Worker<W> {
    writer: W,
    receiver: Receiver<Incoming>,
    next_id: u64,
    refresh_due: bool,
    stopped: bool,
}

impl<W: Write> Worker<W> {
    fn new(writer: W, receiver: Receiver<Incoming>) -> Self {
        Self {
            writer,
            receiver,
            next_id: FIRST_OUTBOUND_ID,
            refresh_due: false,
            stopped: false,
        }
    }

    fn serve(&mut self, inspect: impl Fn() -> HealthReport) -> Result<()> {
        self.publish_health(&inspect(), &inspect)?;
        while !self.stopped {
            match self.receiver.recv() {
                Ok(Incoming::Message(message)) => self.handle_inbound(message, &inspect)?,
                Ok(Incoming::End) | Err(_) => {
                    self.stopped = true;
                    continue;
                }
            }
            if self.refresh_due && !self.stopped {
                self.refresh_due = false;
                self.publish_health(&inspect(), &inspect)?;
            }
        }
        Ok(())
    }

    fn publish_health(
        &mut self,
        report: &HealthReport,
        inspect: &impl Fn() -> HealthReport,
    ) -> Result<()> {
        if self
            .call_host(
                "ui.state.set",
                json!({
                    "slot": "status-bar",
                    "id": "health",
                    "payload": view::status_bar(report)
                }),
                inspect,
            )?
            .is_none()
        {
            return Ok(());
        }
        self.call_host(
            "ui.state.set",
            json!({
                "slot": "settings-page",
                "id": "overview",
                "payload": view::settings_page(report)
            }),
            inspect,
        )?;
        Ok(())
    }

    fn call_host(
        &mut self,
        method: &str,
        params: Value,
        inspect: &impl Fn() -> HealthReport,
    ) -> Result<Option<Value>> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))?;

        loop {
            match self.receiver.recv_timeout(HOST_RPC_TIMEOUT) {
                Ok(Incoming::Message(message))
                    if message.get("id") == Some(&json!(id)) && message.get("method").is_none() =>
                {
                    if let Some(result) = message.get("result") {
                        return Ok(Some(result.clone()));
                    }
                    let code = message
                        .pointer("/error/code")
                        .and_then(Value::as_i64)
                        .unwrap_or(-32603);
                    bail!("AoE host rejected {method} with JSON-RPC code {code}");
                }
                Ok(Incoming::Message(message)) => self.handle_inbound(message, inspect)?,
                Ok(Incoming::End) | Err(RecvTimeoutError::Disconnected) => {
                    self.stopped = true;
                    return Ok(None);
                }
                Err(RecvTimeoutError::Timeout) => {
                    bail!("AoE host did not answer {method} within the bounded timeout")
                }
            }
        }
    }

    fn handle_inbound(&mut self, message: Value, inspect: impl Fn() -> HealthReport) -> Result<()> {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return Ok(());
        };
        let id = message.get("id").cloned();
        match method {
            "praxis.status" => {
                if let Some(id) = id {
                    self.send_response(id, serde_json::to_value(inspect())?)?;
                }
            }
            "praxis.refresh" => {
                if let Some(id) = id {
                    self.send_response(id, json!({"accepted": true}))?;
                }
                self.refresh_due = true;
            }
            "plugin.settings.changed" => {
                if let Some(id) = id {
                    self.send_response(id, json!({"accepted": true}))?;
                }
                self.refresh_due = true;
            }
            "plugin.command.invoke" => {
                let command = message.pointer("/params/command").and_then(Value::as_str);
                if command == Some(STATUS_COMMAND) || command == Some(REFRESH_COMMAND) {
                    if let Some(id) = id {
                        self.send_response(id, json!({"accepted": true}))?;
                    }
                    self.refresh_due = true;
                } else if let Some(id) = id {
                    self.send(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32602,
                            "message": "unknown Praxis plugin command"
                        }
                    }))?;
                }
            }
            _ => {
                if let Some(id) = id {
                    self.send(json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": "unknown Praxis plugin method"
                        }
                    }))?;
                }
            }
        }
        Ok(())
    }

    fn send_response(&mut self, id: Value, result: Value) -> Result<()> {
        self.send(json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    fn send(&mut self, value: Value) -> Result<()> {
        serde_json::to_writer(&mut self.writer, &value).context("serialize AoE plugin message")?;
        self.writer
            .write_all(b"\n")
            .context("write AoE plugin message")?;
        self.writer.flush().context("flush AoE plugin message")
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::application::{ComponentReadiness, HistoryHealth, OverallHealth};

    fn ready_report() -> HealthReport {
        HealthReport {
            status: OverallHealth::Ready,
            key_store: ComponentReadiness::Ready,
            history: HistoryHealth {
                status: ComponentReadiness::Ready,
                event_count: Some(9),
            },
            hooks: vec![],
        }
    }

    #[test]
    fn worker_publishes_health_and_handles_status_and_refresh() {
        let input = [
            json!({"jsonrpc": "2.0", "id": 1_000_000, "result": {"ok": true}}),
            json!({"jsonrpc": "2.0", "id": 1_000_001, "result": {"ok": true}}),
            json!({"jsonrpc": "2.0", "id": 7, "method": "praxis.status", "params": {}}),
            json!({"jsonrpc": "2.0", "id": 8, "method": "praxis.refresh", "params": {}}),
            json!({"jsonrpc": "2.0", "id": 1_000_002, "result": {"ok": true}}),
            json!({"jsonrpc": "2.0", "id": 1_000_003, "result": {"ok": true}}),
        ]
        .into_iter()
        .map(|value| serde_json::to_string(&value).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
        let mut output = Vec::new();

        run(Cursor::new(input), &mut output, ready_report).unwrap();

        let messages: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0]["method"], json!("ui.state.set"));
        assert_eq!(messages[0]["params"]["slot"], json!("status-bar"));
        assert_eq!(messages[1]["params"]["slot"], json!("settings-page"));
        assert_eq!(messages[2]["id"], json!(7));
        assert_eq!(messages[2]["result"]["history"]["event_count"], json!(9));
        assert_eq!(messages[3]["id"], json!(8));
        assert_eq!(messages[3]["result"]["accepted"], json!(true));
        assert_eq!(messages[4]["params"]["slot"], json!("status-bar"));
        assert_eq!(messages[5]["params"]["slot"], json!("settings-page"));
    }

    #[test]
    fn worker_returns_a_bounded_error_for_unknown_methods() {
        let input = [
            json!({"jsonrpc": "2.0", "id": 1_000_000, "result": {"ok": true}}),
            json!({"jsonrpc": "2.0", "id": 1_000_001, "result": {"ok": true}}),
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "PRIVATE_UNKNOWN_METHOD",
                "params": {"PRIVATE": "VALUE"}
            }),
        ]
        .into_iter()
        .map(|value| serde_json::to_string(&value).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
        let mut output = Vec::new();

        run(Cursor::new(input), &mut output, ready_report).unwrap();

        let encoded = String::from_utf8(output).unwrap();
        assert!(encoded.contains(r#""code":-32601"#));
        assert!(!encoded.contains("PRIVATE_UNKNOWN_METHOD"));
        assert!(!encoded.contains("VALUE"));
    }

    #[test]
    fn contributed_commands_refresh_the_published_health() {
        let input = [
            json!({"jsonrpc": "2.0", "id": 1_000_000, "result": {"ok": true}}),
            json!({"jsonrpc": "2.0", "id": 1_000_001, "result": {"ok": true}}),
            json!({
                "jsonrpc": "2.0",
                "method": "plugin.command.invoke",
                "params": {"command": STATUS_COMMAND, "session_id": "opaque-session"}
            }),
            json!({"jsonrpc": "2.0", "id": 1_000_002, "result": {"ok": true}}),
            json!({"jsonrpc": "2.0", "id": 1_000_003, "result": {"ok": true}}),
        ]
        .into_iter()
        .map(|value| serde_json::to_string(&value).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
        let mut output = Vec::new();

        run(Cursor::new(input), &mut output, ready_report).unwrap();

        let messages: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[2]["params"]["slot"], json!("status-bar"));
        assert_eq!(messages[3]["params"]["slot"], json!("settings-page"));
        let encoded = serde_json::to_string(&messages).unwrap();
        assert!(!encoded.contains("opaque-session"));
    }
}
