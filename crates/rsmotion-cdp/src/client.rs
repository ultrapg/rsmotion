use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, warn};

use crate::error::{CdpError, Result};

type PendingReply = oneshot::Sender<std::result::Result<Value, String>>;
type PendingMap = Arc<Mutex<HashMap<u64, PendingReply>>>;

struct OutgoingMsg {
    text: String,
    id: u64,
    reply_tx: PendingReply,
}

/// Async Chrome DevTools Protocol JSON-RPC multiplexer over WebSocket.
pub struct CdpClient {
    next_id: AtomicU64,
    tx: mpsc::UnboundedSender<OutgoingMsg>,
    pending: PendingMap,
    _io_task: JoinHandle<()>,
}

impl CdpClient {
    /// Connects to Chrome's `ws://127.0.0.1:<port>/devtools/browser/...` endpoint.
    pub async fn connect(ws_url: &str) -> Result<Self> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| CdpError::Protocol(format!("WebSocket connect failed: {e}")))?;

        let (mut ws_sink, mut ws_read) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<OutgoingMsg>();
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = Arc::clone(&pending);
        let pending_writer = Arc::clone(&pending);

        let io_task = tokio::spawn(async move {
            let writer_fut = async {
                while let Some(msg) = rx.recv().await {
                    {
                        let mut guard = pending_writer.lock().await;
                        guard.insert(msg.id, msg.reply_tx);
                    }
                    if let Err(e) = ws_sink.send(Message::text(msg.text)).await {
                        debug!("CDP write error: {e}");
                        let mut guard = pending_writer.lock().await;
                        if let Some(reply) = guard.remove(&msg.id) {
                            let _ = reply.send(Err(format!("WebSocket send error: {e}")));
                        }
                        break;
                    }
                }
            };

            let reader_fut = async {
                while let Some(item) = ws_read.next().await {
                    match item {
                        Ok(Message::Text(txt)) => {
                            let val: Value = match serde_json::from_str(txt.as_str()) {
                                Ok(v) => v,
                                Err(e) => {
                                    warn!("Invalid JSON from CDP: {e}");
                                    continue;
                                }
                            };
                            if let Some(id) = val.get("id").and_then(|v| v.as_u64()) {
                                let reply = pending_reader.lock().await.remove(&id);
                                if let Some(tx) = reply {
                                    if let Some(err) = val.get("error") {
                                        let msg = err
                                            .get("message")
                                            .and_then(|m| m.as_str())
                                            .unwrap_or("unknown CDP error")
                                            .to_string();
                                        let _ = tx.send(Err(msg));
                                    } else {
                                        let res = val.get("result").cloned().unwrap_or(Value::Null);
                                        let _ = tx.send(Ok(res));
                                    }
                                }
                            }
                        }
                        Ok(Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(e) => {
                            debug!("CDP read error: {e}");
                            break;
                        }
                    }
                }

                // Fail any still-waiting callers so they don't wait for the full timeout.
                let mut guard = pending_reader.lock().await;
                for (_, tx) in guard.drain() {
                    let _ = tx.send(Err("CDP WebSocket closed".to_string()));
                }
            };

            tokio::select! {
                _ = writer_fut => {}
                _ = reader_fut => {}
            }
        });

        Ok(Self {
            next_id: AtomicU64::new(1),
            tx,
            pending,
            _io_task: io_task,
        })
    }

    async fn call_raw(
        &self,
        session_id: Option<&str>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut payload = json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(sid) = session_id {
            payload["sessionId"] = Value::String(sid.to_string());
        }
        let text = serde_json::to_string(&payload)?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(OutgoingMsg { text, id, reply_tx })
            .map_err(|_| CdpError::Protocol("CDP sender channel closed".to_string()))?;

        match tokio::time::timeout(timeout, reply_rx).await {
            Ok(Ok(Ok(val))) => Ok(val),
            Ok(Ok(Err(message))) => Err(CdpError::CdpRemote {
                method: method.to_string(),
                message,
            }),
            Ok(Err(_)) => Err(CdpError::Protocol(
                "CDP response channel dropped".to_string(),
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(CdpError::CommandTimeout {
                    method: method.to_string(),
                    timeout_ms: timeout.as_millis() as u64,
                })
            }
        }
    }

    /// Sends a browser-level CDP command.
    pub async fn send(&self, method: &str, params: Value) -> Result<Value> {
        self.call_raw(None, method, params, Duration::from_secs(30))
            .await
    }

    /// Sends a page-session-level CDP command with an explicit timeout.
    pub async fn send_session_timeout(
        &self,
        session_id: &str,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        self.call_raw(Some(session_id), method, params, timeout)
            .await
    }

    /// Sends a page-session-level CDP command (30s default timeout).
    pub async fn send_session(
        &self,
        session_id: &str,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        self.send_session_timeout(session_id, method, params, Duration::from_secs(30))
            .await
    }

    /// Opens a fresh `about:blank` target and returns `(target_id, session_id)`.
    pub async fn create_page_session(&self) -> Result<(String, String)> {
        let created = self
            .send("Target.createTarget", json!({ "url": "about:blank" }))
            .await?;
        let target_id = created
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CdpError::Protocol("missing targetId from Target.createTarget".into()))?
            .to_string();

        let attached = self
            .send(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
            )
            .await?;
        let session_id = attached
            .get("sessionId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CdpError::Protocol("missing sessionId from Target.attachToTarget".into())
            })?
            .to_string();

        Ok((target_id, session_id))
    }

    /// Closes a page target cleanly.
    pub async fn close_target(&self, target_id: &str) {
        let _ = self
            .send("Target.closeTarget", json!({ "targetId": target_id }))
            .await;
    }
}
