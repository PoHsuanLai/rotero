use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

use super::helpers::{first_allow_option_id, handle_notification};
use super::node::find_or_install_node;
use super::types::ChatEvent;

pub(crate) struct RawAcpConnection {
    pub(crate) child: Child,
    pub(crate) incoming: mpsc::Receiver<String>,
    pub(crate) next_id: u64,
}

impl RawAcpConnection {
    pub(crate) fn spawn(entry_point: &PathBuf, extra_args: &[&str]) -> Result<Self, String> {
        let node_bin = find_or_install_node()?;
        let mut cmd = Command::new(&node_bin);
        cmd.arg(entry_point);
        for arg in extra_args {
            cmd.arg(arg);
        }
        let config = crate::sync::engine::SyncConfig::load();
        for (key, val) in &config.agent.agent_api_keys {
            if !val.is_empty() {
                cmd.env(key, val);
            }
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn node: {e}"))?;
        let stdout = child.stdout.take().ok_or("No stdout")?;

        let (line_tx, line_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("acp-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if line_tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn reader thread: {e}"))?;

        Ok(Self {
            child,
            incoming: line_rx,
            next_id: 1,
        })
    }

    pub(crate) fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
        evt_tx: Option<&tokio::sync::mpsc::UnboundedSender<ChatEvent>>,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.write_message(&msg)?;

        loop {
            let line = self
                .incoming
                .recv()
                .map_err(|_| "Connection closed".to_string())?;

            let v: serde_json::Value =
                serde_json::from_str(line.trim()).map_err(|e| format!("Parse error: {e}"))?;

            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                if let Some(error) = v.get("error") {
                    return Err(format!("RPC error: {error}"));
                }
                return v
                    .get("result")
                    .cloned()
                    .ok_or("No result in response".into());
            }

            if v.get("method").and_then(|m| m.as_str()) == Some("session/request_permission") {
                if let Some(req_id) = v.get("id") {
                    let option_id = first_allow_option_id(&v);
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": { "outcome": { "outcome": "selected", "optionId": option_id } }
                    });
                    let _ = self.write_message(&response);
                }
                continue;
            }

            if let Some(tx) = evt_tx {
                handle_notification(tx, &v);
            }
        }
    }

    pub(crate) fn write_message(&mut self, msg: &serde_json::Value) -> Result<(), String> {
        let stdin = self.child.stdin.as_mut().ok_or("No stdin")?;
        let line = serde_json::to_string(msg).map_err(|e| format!("JSON error: {e}"))?;
        stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("Write error: {e}"))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| format!("Write error: {e}"))?;
        stdin.flush().map_err(|e| format!("Flush error: {e}"))?;
        Ok(())
    }

    pub(crate) fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&msg)
    }

    pub(crate) fn try_read_line(&mut self) -> Option<String> {
        self.incoming.try_recv().ok()
    }

    pub(crate) fn kill(&mut self) {
        #[cfg(unix)]
        {
            let pid = self.child.id();
            unsafe {
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Drop for RawAcpConnection {
    fn drop(&mut self) {
        self.kill();
    }
}
