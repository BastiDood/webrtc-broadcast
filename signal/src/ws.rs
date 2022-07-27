//! WebSocket-related utilities.

use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

/// See [Client Handshake Request](https://developer.mozilla.org/en-US/docs/Web/API/WebSockets_API/Writing_WebSocket_servers#client_handshake_request).
/// Returns the derived accept key and the WebRTC offer of the remote.
pub fn validate_headers(headers: &hyper::HeaderMap) -> Option<(Box<str>, RTCSessionDescription)> {
    use hyper::header;

    if headers.get(header::CONNECTION)?.as_bytes() != b"Upgrade"
        || headers.get(header::UPGRADE)?.as_bytes() != b"websocket"
        || headers.get(header::SEC_WEBSOCKET_VERSION)?.as_bytes() != b"13"
    {
        return None;
    }

    let offer = headers.get(header::SEC_WEBSOCKET_PROTOCOL)?.as_bytes();
    let desc = serde_json::from_slice(offer).ok()?;

    use tokio_tungstenite::tungstenite::handshake;
    let key = headers.get(header::SEC_WEBSOCKET_KEY)?.as_bytes();
    let derived = handshake::derive_accept_key(key).into_boxed_str();
    Some((derived, desc))
}
