//! Network transport abstraction.
//!
//! The Rysk client depends on this trait rather than concrete HTTP and websocket
//! libraries, so request building and response mapping can be tested without
//! network access (see `tests/rysk_flow.rs`).

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::{Message, http};

const USER_AGENT: &str = concat!("rysk-mcp/", env!("CARGO_PKG_VERSION"));
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounds connect, send and the wait for the answer. An RFQ is an auction, so the
/// answer can take a few seconds to arrive.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// Upstream error bodies are short messages; cap them so a misrouted HTML page
/// cannot flood the tool output.
const MAX_ERROR_BODY: usize = 300;

/// An error from the underlying transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportError(pub String);

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TransportError {}

/// The two ways the Rysk API is reached: plain HTTP GETs for public data, and
/// JSON-RPC over the taker websocket for positions and quotes.
#[allow(async_fn_in_trait)]
pub trait Transport {
    /// GET `url` and return the body. A non-success status is an error that
    /// carries the status and the body, which is where Rysk puts its message.
    async fn get(&self, url: &str) -> Result<String, TransportError>;

    /// Open a websocket to `url`, send one JSON-RPC `request`, and return the
    /// first text message whose `id` equals `id`.
    async fn rpc(&self, url: &str, id: &str, request: &str) -> Result<String, TransportError>;
}

/// Allow a shared reference to a transport to be used as a transport, so a test
/// can hand a transport to a client and still inspect it.
impl<T: Transport> Transport for &T {
    async fn get(&self, url: &str) -> Result<String, TransportError> {
        (**self).get(url).await
    }

    async fn rpc(&self, url: &str, id: &str, request: &str) -> Result<String, TransportError> {
        (**self).rpc(url, id, request).await
    }
}

/// Production transport: `reqwest` for HTTP and `tokio-tungstenite` for the
/// websocket, both over rustls.
#[derive(Clone)]
pub struct NetTransport {
    client: reqwest::Client,
}

impl NetTransport {
    pub fn new() -> Result<Self, TransportError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()
            .map_err(|e| TransportError(format!("building HTTP client: {e}")))?;
        Ok(Self { client })
    }
}

impl Transport for NetTransport {
    async fn get(&self, url: &str) -> Result<String, TransportError> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TransportError(format!("request failed: {}", e.without_url())))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| TransportError(format!("reading response body: {}", e.without_url())))?;
        if !status.is_success() {
            return Err(TransportError(format!(
                "HTTP {status}: {}",
                truncate(body.trim(), MAX_ERROR_BODY)
            )));
        }
        Ok(body)
    }

    async fn rpc(&self, url: &str, id: &str, request: &str) -> Result<String, TransportError> {
        tokio::time::timeout(RPC_TIMEOUT, rpc_once(url, id, request))
            .await
            .map_err(|_| {
                TransportError(format!(
                    "no answer from Rysk within {}s",
                    RPC_TIMEOUT.as_secs()
                ))
            })?
    }
}

async fn rpc_once(url: &str, id: &str, request: &str) -> Result<String, TransportError> {
    let mut handshake = url
        .into_client_request()
        .map_err(|e| TransportError(format!("invalid websocket URL: {e}")))?;
    handshake.headers_mut().insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(USER_AGENT),
    );
    let (mut socket, _) = tokio_tungstenite::connect_async(handshake)
        .await
        .map_err(|e| TransportError(format!("websocket connect failed: {e}")))?;
    socket
        .send(Message::text(request))
        .await
        .map_err(|e| TransportError(format!("websocket send failed: {e}")))?;

    while let Some(message) = socket.next().await {
        let message = message.map_err(|e| TransportError(format!("websocket read failed: {e}")))?;
        match message {
            Message::Text(text) if response_id(&text).as_deref() == Some(id) => {
                // The answer is in hand; a failed close handshake changes nothing.
                let _ = socket.close(None).await;
                return Ok(text.to_string());
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    Err(TransportError(
        "websocket closed before Rysk answered".to_string(),
    ))
}

fn response_id(text: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        id: Option<serde_json::Value>,
    }
    match serde_json::from_str::<Envelope>(text).ok()?.id? {
        serde_json::Value::String(id) => Some(id),
        other => Some(other.to_string()),
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_id_reads_string_and_numeric_ids() {
        assert_eq!(
            response_id(r#"{"jsonrpc":"2.0","id":"positions","result":[]}"#).as_deref(),
            Some("positions")
        );
        assert_eq!(response_id(r#"{"id":7}"#).as_deref(), Some("7"));
        assert_eq!(response_id(r#"{"method":"notify"}"#), None);
        assert_eq!(response_id("not json"), None);
    }

    #[test]
    fn truncate_caps_long_bodies_on_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("ééééé", 2), "éé…");
    }
}
