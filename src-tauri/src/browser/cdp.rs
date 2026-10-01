//! Minimal Chrome DevTools Protocol client: JSON-RPC over one WebSocket.
//! Calls are matched to responses by id; events go to a channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

const CALL_TIMEOUT: Duration = Duration::from_secs(30);

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

/// A CDP event: (method, params).
pub type CdpEvent = (String, Value);

pub struct Cdp {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
}

impl Cdp {
    /// Connects to a DevTools WebSocket (browser or page endpoint).
    pub async fn connect(url: &str) -> Result<(Arc<Cdp>, mpsc::UnboundedReceiver<CdpEvent>), String> {
        let (ws, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| format!("cannot connect to the browser ({})", e))?;
        let (mut sink, mut stream) = ws.split();

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(text) = out_rx.recv().await {
                if sink.send(Message::text(text)).await.is_err() {
                    break;
                }
            }
        });

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (ev_tx, ev_rx) = mpsc::unbounded_channel::<CdpEvent>();
        let pending_reader = Arc::clone(&pending);
        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                let Message::Text(text) = msg else { continue };
                let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    let tx = pending_reader.lock().unwrap().remove(&id);
                    if let Some(tx) = tx {
                        let res = match v.get("error") {
                            Some(err) => Err(err.get("message").and_then(Value::as_str).unwrap_or("CDP error").to_string()),
                            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(res);
                    }
                } else if let Some(method) = v.get("method").and_then(Value::as_str) {
                    let _ = ev_tx.send((method.to_string(), v.get("params").cloned().unwrap_or(Value::Null)));
                }
            }
            // Connection gone: every waiting call fails now instead of timing out
            for (_, tx) in pending_reader.lock().unwrap().drain() {
                let _ = tx.send(Err("the browser connection closed".into()));
            }
        });

        Ok((Arc::new(Cdp { out: out_tx, pending, next_id: AtomicU64::new(1) }), ev_rx))
    }

    fn send(&self, id: u64, method: &str, params: Value) -> Result<(), String> {
        let msg = json!({ "id": id, "method": method, "params": params }).to_string();
        self.out.send(msg).map_err(|_| "the browser connection closed".to_string())
    }

    /// Sends a command and waits for its result.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        if let Err(e) = self.send(id, method, params) {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match tokio::time::timeout(CALL_TIMEOUT, rx).await {
            Ok(Ok(res)) => res.map_err(|e| format!("{}: {}", method, e)),
            Ok(Err(_)) => Err("the browser connection closed".into()),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(format!("{}: the browser did not answer in {}s", method, CALL_TIMEOUT.as_secs()))
            }
        }
    }

    /// Fire-and-forget command (screencast acks, input during drags).
    pub fn notify(&self, method: &str, params: Value) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.send(id, method, params);
    }
}
