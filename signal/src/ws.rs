use alloc::boxed::Box;
use hyper::HeaderMap;

/// See [Client Handshake Request](https://developer.mozilla.org/en-US/docs/Web/API/WebSockets_API/Writing_WebSocket_servers#client_handshake_request).
/// Returns the derived accept key for the
/// [`Sec-WebSocket-Accept`](hyper::header::SEC_WEBSOCKET_ACCEPT) header.
pub fn validate_headers(headers: &HeaderMap) -> Option<Box<str>> {
    use hyper::header;

    if headers.get(header::CONNECTION)?.as_bytes() != b"Upgrade" {
        return None;
    }

    if headers.get(header::UPGRADE)?.as_bytes() != b"websocket" {
        return None;
    }

    if headers.get(header::SEC_WEBSOCKET_VERSION)?.as_bytes() != b"13" {
        return None;
    }

    use tokio_tungstenite::tungstenite::handshake;
    let key = headers.get(header::SEC_WEBSOCKET_KEY)?.as_bytes();
    Some(handshake::derive_accept_key(key).into_boxed_str())
}
