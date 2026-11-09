//! Tests for the real network transport, against local HTTP and websocket servers.

use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use rysk_mcp::rysk::transport::{NetTransport, Transport};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn get_returns_the_body_and_identifies_the_client() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/assets"))
        .and(header_regex("user-agent", r"^rysk-mcp/\d+\.\d+\.\d+$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"999":[]}"#))
        .mount(&server)
        .await;

    let body = NetTransport::new()
        .unwrap()
        .get(&format!("{}/api/assets", server.uri()))
        .await
        .unwrap();

    assert_eq!(body, r#"{"999":[]}"#);
}

#[tokio::test]
async fn get_errors_carry_status_and_rysk_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_string("unknown asset\n"))
        .mount(&server)
        .await;

    let err = NetTransport::new()
        .unwrap()
        .get(&format!("{}/api/expiry-prices/999/0x1", server.uri()))
        .await
        .unwrap_err();

    assert_eq!(err.to_string(), "HTTP 404 Not Found: unknown asset");
}

#[tokio::test]
async fn get_errors_truncate_long_bodies() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string("x".repeat(1000)))
        .mount(&server)
        .await;

    let err = NetTransport::new()
        .unwrap()
        .get(&server.uri())
        .await
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        format!("HTTP 500 Internal Server Error: {}…", "x".repeat(300))
    );
}

/// Accept one websocket client, record its request and user agent, then run `script`.
// tungstenite fixes the handshake callback's large `Err` type; it is not ours to shrink.
#[allow(clippy::result_large_err)]
async fn serve_once<F, Fut>(script: F) -> (String, Arc<Mutex<Option<(String, String)>>>)
where
    F: FnOnce(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/taker", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(None));
    let record = Arc::clone(&seen);
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let user_agent = Arc::new(Mutex::new(String::new()));
        let ua = Arc::clone(&user_agent);
        let mut socket = tokio_tungstenite::accept_hdr_async(
            stream,
            move |request: &Request, response: Response| {
                *ua.lock().unwrap() = request
                    .headers()
                    .get("user-agent")
                    .map(|v| v.to_str().unwrap().to_string())
                    .unwrap_or_default();
                Ok(response)
            },
        )
        .await
        .unwrap();
        let Some(Ok(Message::Text(request))) = socket.next().await else {
            panic!("expected a text request");
        };
        let user_agent = user_agent.lock().unwrap().clone();
        *record.lock().unwrap() = Some((request.to_string(), user_agent));
        script(socket).await;
    });
    (url, seen)
}

#[tokio::test]
async fn rpc_returns_the_answer_with_the_matching_id() {
    let (url, seen) = serve_once(|mut socket| async move {
        socket
            .send(Message::text(
                r#"{"jsonrpc":"2.0","method":"inventory","params":{}}"#,
            ))
            .await
            .unwrap();
        socket.send(Message::Ping(vec![1].into())).await.unwrap();
        socket
            .send(Message::text(
                r#"{"jsonrpc":"2.0","id":"other","result":1}"#,
            ))
            .await
            .unwrap();
        socket
            .send(Message::text(
                r#"{"jsonrpc":"2.0","id":"positions","result":[]}"#,
            ))
            .await
            .unwrap();
        // Let the client close; it must not wait for anything else.
        while socket.next().await.is_some() {}
    })
    .await;

    let answer = NetTransport::new()
        .unwrap()
        .rpc(&url, "positions", r#"{"id":"positions"}"#)
        .await
        .unwrap();

    assert_eq!(answer, r#"{"jsonrpc":"2.0","id":"positions","result":[]}"#);
    let (request, user_agent) = seen.lock().unwrap().clone().unwrap();
    assert_eq!(request, r#"{"id":"positions"}"#);
    assert!(user_agent.starts_with("rysk-mcp/"), "{user_agent}");
}

#[tokio::test]
async fn rpc_fails_when_the_server_closes_without_answering() {
    let (url, _) = serve_once(|mut socket| async move {
        socket.close(None).await.unwrap();
    })
    .await;

    let err = NetTransport::new()
        .unwrap()
        .rpc(&url, "positions", "{}")
        .await
        .unwrap_err();

    assert_eq!(err.to_string(), "websocket closed before Rysk answered");
}

#[tokio::test]
async fn rpc_fails_when_nothing_listens() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/taker", listener.local_addr().unwrap());
    drop(listener);

    let err = NetTransport::new()
        .unwrap()
        .rpc(&url, "positions", "{}")
        .await
        .unwrap_err();

    assert!(
        err.to_string().starts_with("websocket connect failed:"),
        "{err}"
    );
}
