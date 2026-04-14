use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use crate::TranslateError;
use crate::translator::ZoteroItem;

/// Manages a Zotero translation-server subprocess.
pub struct TranslationServer {
    port: u16,
    child: Mutex<Option<Child>>,
    server_dir: PathBuf,
    node_bin: PathBuf,
    npm_bin: PathBuf,
}

impl TranslationServer {
    /// Create a new translation server manager.
    /// `node_bin` and `npm_bin` are resolved by the caller (e.g. via auto-download).
    /// Call `ensure_running()` before making requests.
    pub fn new(port: u16, node_bin: PathBuf, npm_bin: PathBuf) -> Self {
        let base = directories::BaseDirs::new()
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let server_dir = base.join("com.rotero.Rotero").join("translation-server");

        Self {
            port,
            child: Mutex::new(None),
            server_dir,
            node_bin,
            npm_bin,
        }
    }

    /// Install translation-server via npm if not present, then start it.
    pub async fn ensure_running(&self) -> Result<(), TranslateError> {
        // Check if already running
        if self.is_healthy().await {
            return Ok(());
        }

        // Install if needed (clone + npm install can take minutes on first run)
        tracing::info!("Ensuring translation-server is installed...");
        self.install().await?;

        // Start the server
        self.start()?;

        // Wait for it to be ready (up to 60 seconds — first startup loads 700+ translators)
        for i in 0..120 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Check if the process exited early
            {
                let mut guard = self.child.lock().unwrap();
                if let Some(ref mut child) = *guard {
                    if let Ok(Some(status)) = child.try_wait() {
                        let stderr = child
                            .stderr
                            .take()
                            .and_then(|mut s| {
                                let mut buf = String::new();
                                std::io::Read::read_to_string(&mut s, &mut buf).ok()?;
                                Some(buf)
                            })
                            .unwrap_or_default();
                        tracing::error!(
                            "Translation server exited with {status} after ~{}s. stderr:\n{stderr}",
                            i / 2
                        );
                        *guard = None;
                        return Err(TranslateError::Setup(format!(
                            "Translation server exited with {status}: {stderr}"
                        )));
                    }
                }
            }

            if self.is_healthy().await {
                tracing::info!(
                    "Translation server ready on port {} (took ~{}s)",
                    self.port,
                    i / 2
                );
                return Ok(());
            }
        }

        // Timed out — grab stderr for diagnostics
        let stderr = {
            let mut guard = self.child.lock().unwrap();
            if let Some(ref mut child) = *guard {
                let _ = child.kill();
                child
                    .stderr
                    .take()
                    .and_then(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok()?;
                        Some(buf)
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            }
        };
        tracing::error!("Translation server failed to start within 60s. stderr:\n{stderr}");
        Err(TranslateError::Setup(format!(
            "Translation server failed to start within 60 seconds: {stderr}"
        )))
    }

    /// Check whether the translation server is responding on its HTTP port.
    async fn is_healthy(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/", self.port);
        reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .is_ok()
    }

    /// Clone and set up the translation-server repo if not already present.
    async fn install(&self) -> Result<(), TranslateError> {
        let server_js = self.server_dir.join("src").join("server.js");
        if server_js.exists() {
            // Already installed — just make sure deps are present
            let node_modules = self.server_dir.join("node_modules");
            if !node_modules.exists() {
                self.npm_install()?;
            }
            return Ok(());
        }

        tracing::info!("Cloning translation-server from GitHub...");

        // Clone the repo (shallow)
        let output = Command::new("git")
            .args([
                "clone",
                "--depth=1",
                "https://github.com/zotero/translation-server.git",
                &self.server_dir.to_string_lossy(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| TranslateError::Setup(format!("git clone failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TranslateError::Setup(format!(
                "git clone translation-server failed: {stderr}"
            )));
        }

        // Initialize submodules separately (--recurse-submodules can be unreliable with --depth=1)
        tracing::info!("Initializing submodules (translators, utilities, translate)...");
        let output = Command::new("git")
            .args(["submodule", "update", "--init", "--depth=1"])
            .current_dir(&self.server_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| TranslateError::Setup(format!("git submodule update failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TranslateError::Setup(format!(
                "git submodule update failed: {stderr}"
            )));
        }

        // npm install dependencies
        self.npm_install()?;

        // Patch: preserve attachment URLs in translation output.
        // Stock translation-server uses a no-op ItemSaver, so the translate engine's
        // cleanup strips attachments that weren't "downloaded". We add a saveItems
        // method that marks all attachments as in-progress so they survive cleanup.
        self.patch_preserve_attachments()?;

        tracing::info!("translation-server installed");
        Ok(())
    }

    /// Patch translate_item.js to preserve attachment URLs in output.
    fn patch_preserve_attachments(&self) -> Result<(), TranslateError> {
        let file = self
            .server_dir
            .join("src")
            .join("translation")
            .join("translate_item.js");

        let content = std::fs::read_to_string(&file)
            .map_err(|e| TranslateError::Setup(format!("Failed to read translate_item.js: {e}")))?;

        // Only patch if not already patched
        if content.contains("ROTERO_PATCHED") {
            return Ok(());
        }

        // Add saveItems that reports all attachments as "in progress" via attachmentCallback
        let patch = r#"
// ROTERO_PATCHED: preserve attachment URLs in output
ItemSaver.prototype.saveItems = async function (jsonItems, attachmentCallback, itemsDoneCallback) {
    this.items = (this.items || []).concat(jsonItems);
    // Report each attachment as "in progress" so it survives the cleanup
    if (attachmentCallback) {
        for (var i = 0; i < jsonItems.length; i++) {
            var atts = jsonItems[i].attachments;
            if (atts) {
                for (var j = 0; j < atts.length; j++) {
                    try { attachmentCallback(atts[j], 100); } catch(e) {}
                }
            }
        }
    }
    return jsonItems;
};
"#;

        let patched = content.clone() + patch;
        std::fs::write(&file, patched).map_err(|e| {
            TranslateError::Setup(format!("Failed to patch translate_item.js: {e}"))
        })?;

        tracing::info!("Patched translation-server to preserve attachment URLs");
        Ok(())
    }

    /// Run `npm install --production` in the translation-server directory.
    fn npm_install(&self) -> Result<(), TranslateError> {
        tracing::info!("Running npm install for translation-server...");

        let output = Command::new(&self.npm_bin)
            .args(["install", "--production"])
            .current_dir(&self.server_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| TranslateError::Setup(format!("npm install failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TranslateError::Setup(format!(
                "npm install failed: {stderr}"
            )));
        }
        Ok(())
    }

    /// Spawn the Node.js translation-server process on the configured port.
    fn start(&self) -> Result<(), TranslateError> {
        let entry = self.server_dir.join("src").join("server.js");

        if !entry.exists() {
            return Err(TranslateError::Setup(format!(
                "Server entry point not found: {}",
                entry.display()
            )));
        }

        tracing::info!(
            "Starting translation-server on port {} (node: {})",
            self.port,
            self.node_bin.display()
        );

        // Use NODE_CONFIG env var to override port (config npm module convention)
        let node_config = format!(r#"{{"port":{}}}"#, self.port);

        let child = Command::new(&self.node_bin)
            .arg(&entry)
            .current_dir(&self.server_dir)
            .env("NODE_CONFIG", &node_config)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TranslateError::Setup(format!("Failed to spawn server: {e}")))?;

        let mut guard = self.child.lock().unwrap();
        *guard = Some(child);
        Ok(())
    }

    /// Kill the translation-server child process if it is running.
    pub fn stop(&self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(ref mut child) = *guard {
            let _ = child.kill();
            let _ = child.wait();
        }
        *guard = None;
    }

    /// Return the HTTP base URL for the translation server.
    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// POST /web — translate a URL into metadata.
    pub async fn translate_web(&self, url: &str) -> Result<Vec<ZoteroItem>, TranslateError> {
        let resp = reqwest::Client::new()
            .post(format!("{}/web", self.base_url()))
            .header("Content-Type", "text/plain")
            .body(url.to_string())
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        match status {
            200 => {
                let items: Vec<ZoteroItem> = serde_json::from_str(&body)
                    .map_err(|e| TranslateError::Translation(format!("Parse error: {e}")))?;
                if items.is_empty() {
                    Err(TranslateError::NoResults)
                } else {
                    Ok(items)
                }
            }
            300 => {
                // Multiple choices — auto-select all
                let multi: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|e| TranslateError::Translation(e.to_string()))?;

                let session = multi.get("session").and_then(|v| v.as_str()).unwrap_or("");
                let items_map = multi
                    .get("items")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();

                // Select all items
                let mut selected = serde_json::Map::new();
                for key in items_map.keys() {
                    selected.insert(key.clone(), serde_json::Value::Bool(true));
                }

                let selection_body = serde_json::json!({
                    "session": session,
                    "items": selected,
                });

                let resp2 = reqwest::Client::new()
                    .post(format!("{}/web", self.base_url()))
                    .header("Content-Type", "application/json")
                    .json(&selection_body)
                    .timeout(std::time::Duration::from_secs(30))
                    .send()
                    .await
                    .map_err(|e| TranslateError::Http(e.to_string()))?;

                if resp2.status().is_success() {
                    let body2 = resp2
                        .text()
                        .await
                        .map_err(|e| TranslateError::Http(e.to_string()))?;
                    let items: Vec<ZoteroItem> = serde_json::from_str(&body2)
                        .map_err(|e| TranslateError::Translation(format!("Parse error: {e}")))?;
                    Ok(items)
                } else {
                    Err(TranslateError::Translation(format!(
                        "Selection failed: HTTP {}",
                        resp2.status()
                    )))
                }
            }
            _ => Err(TranslateError::Translation(format!(
                "HTTP {status}: {body}"
            ))),
        }
    }

    /// POST /search — look up by identifier (DOI, ISBN, PMID).
    pub async fn search(&self, identifier: &str) -> Result<Vec<ZoteroItem>, TranslateError> {
        let resp = reqwest::Client::new()
            .post(format!("{}/search", self.base_url()))
            .header("Content-Type", "text/plain")
            .body(identifier.to_string())
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        match status {
            200 => {
                let items: Vec<ZoteroItem> = serde_json::from_str(&body)
                    .map_err(|e| TranslateError::Translation(format!("Parse error: {e}")))?;
                Ok(items)
            }
            _ => Err(TranslateError::Translation(format!(
                "HTTP {status}: {body}"
            ))),
        }
    }

    /// POST /import — parse bibliography text (BibTeX, RIS, etc.).
    pub async fn import(&self, text: &str) -> Result<Vec<ZoteroItem>, TranslateError> {
        let resp = reqwest::Client::new()
            .post(format!("{}/import", self.base_url()))
            .header("Content-Type", "text/plain")
            .body(text.to_string())
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        match status {
            200 => {
                let items: Vec<ZoteroItem> = serde_json::from_str(&body)
                    .map_err(|e| TranslateError::Translation(format!("Parse error: {e}")))?;
                Ok(items)
            }
            _ => Err(TranslateError::Translation(format!(
                "HTTP {status}: {body}"
            ))),
        }
    }

    /// POST /export — convert items to bibliography format.
    pub async fn export(
        &self,
        items: &[ZoteroItem],
        format: &str,
    ) -> Result<String, TranslateError> {
        let resp = reqwest::Client::new()
            .post(format!("{}/export?format={format}", self.base_url()))
            .header("Content-Type", "application/json")
            .json(items)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| TranslateError::Http(e.to_string()))?;

        match status {
            200 => Ok(body),
            _ => Err(TranslateError::Translation(format!(
                "HTTP {status}: {body}"
            ))),
        }
    }
}

impl Drop for TranslationServer {
    fn drop(&mut self) {
        self.stop();
    }
}
