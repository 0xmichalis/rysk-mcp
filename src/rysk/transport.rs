//! Network transport abstraction.
//!
//! The Rysk client depends on this trait rather than concrete HTTP and websocket
//! libraries, so request building and response mapping can be tested without
//! network access (see `tests/rysk_flow.rs`).

use std::{error::Error, time::Duration};

use futures_util::{FutureExt, SinkExt, StreamExt};
use http_body_util::{LengthLimitError, Limited};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::{Message, http};

const USER_AGENT: &str = concat!("rysk-mcp/", env!("CARGO_PKG_VERSION"));
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Bounds connect, send and the wait for the answer. An RFQ is an auction, so the
/// answer can take a few seconds to arrive.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// Match tungstenite's existing message limit and give inventory and position
/// histories the same budget across transports.
const PROVIDER_RESPONSE_BYTES_MAX: usize = 64 * 1024 * 1024;
/// One RPC expects one answer. Bound the count as well as the combined size of
/// preceding messages, including control messages with empty payloads.
const RPC_MESSAGES_MAX: u32 = 4096;
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
    /// carries the status and a bounded preview of the body, where Rysk puts its message.
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
        let body = read_http_text(response, PROVIDER_RESPONSE_BYTES_MAX).await?;
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

async fn read_http_text(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<String, TransportError> {
    let status = response.status();
    let (parts, body) = http::Response::from(response).into_parts();
    // Keep reqwest's timeout and text decoding while limiting the body it collects.
    let body = reqwest::Body::wrap(Limited::new(body, max_bytes));
    let response = reqwest::Response::from(http::Response::from_parts(parts, body));
    response.text().await.map_err(|error| {
        if std::iter::successors(error.source(), |&source| source.source())
            .any(|source| source.is::<LengthLimitError>())
        {
            TransportError(format!(
                "HTTP {status}: response body exceeds {max_bytes} bytes"
            ))
        } else {
            TransportError(format!("reading response body: {}", error.without_url()))
        }
    })
}

async fn rpc_once(url: &str, id: &str, request: &str) -> Result<String, TransportError> {
    let mut handshake = url
        .into_client_request()
        .map_err(|e| TransportError(format!("invalid websocket URL: {e}")))?;
    handshake.headers_mut().insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(USER_AGENT),
    );
    let config = WebSocketConfig::default().max_message_size(Some(PROVIDER_RESPONSE_BYTES_MAX));
    let (mut socket, _) =
        tokio_tungstenite::connect_async_with_config(handshake, Some(config), false)
            .await
            .map_err(|e| TransportError(format!("websocket connect failed: {e}")))?;
    socket
        .send(Message::text(request))
        .await
        .map_err(|e| TransportError(format!("websocket send failed: {e}")))?;

    let mut unrelated_bytes_left = PROVIDER_RESPONSE_BYTES_MAX;
    for _ in 0..RPC_MESSAGES_MAX {
        let message = socket
            .next()
            .await
            .transpose()
            .map_err(|e| TransportError(format!("websocket read failed: {e}")))?;
        match message {
            Some(Message::Text(text)) if response_id(&text).as_deref() == Some(id) => {
                // Send close if the socket is writable without delaying the answer.
                let _ = socket.close(None).now_or_never();
                return Ok(text.to_string());
            }
            Some(Message::Close(_)) | None => {
                return Err(TransportError(
                    "websocket closed before Rysk answered".to_string(),
                ));
            }
            Some(other) => {
                unrelated_bytes_left =
                    unrelated_bytes_left.checked_sub(other.len()).ok_or_else(|| {
                        TransportError(format!(
                            "websocket exceeded {PROVIDER_RESPONSE_BYTES_MAX} bytes before Rysk answered"
                        ))
                    })?;
            }
        }
    }
    Err(TransportError(format!(
        "websocket received {RPC_MESSAGES_MAX} messages before Rysk answered"
    )))
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn http_body_at_byte_limit_keeps_charset_decoding() {
        let response = http::Response::builder()
            .header(
                http::header::CONTENT_TYPE,
                "text/plain; charset=windows-1252",
            )
            .body(b"caf\xe9".to_vec())
            .unwrap();

        let text = read_http_text(response.into(), 4).await.unwrap();

        assert_eq!(text, "café");
    }

    #[tokio::test]
    async fn http_body_limit_counts_across_chunks_without_content_length() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0; 1024];
            assert_ne!(stream.read(&mut request).await.unwrap(), 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n")
                .await
                .unwrap();
        });

        let response = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap();
        assert_eq!(response.content_length(), None);
        let error = read_http_text(response, 4).await.unwrap_err();

        assert_eq!(error.0, "HTTP 200 OK: response body exceeds 4 bytes");
        server.await.unwrap();
    }

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
