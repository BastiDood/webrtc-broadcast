# The Back-end
Given the performance sensitivity of media streaming, the intermediary server is written in [Rust]. By default, the server executable binds to the socket address `0.0.0.0:3000`.

Building on the shoulders of giants, the back-end directly depends on the following key dependencies:

* The current implementation uses the powerful [Tokio] runtime to handle many concurrent requests over multiple threads.
* On top of [Tokio], the [Hyper] framework provides a "fast and correct" HTTP implementation.
* For the WebSocket protocol (used for signaling), the server depends on the [Tungstenite] project.
* Signalled JSON messages (via WebSockets) are then serialized and deserialized with the [`serde_json`] crate via the [Serde] framework.
* Finally, the [WebRTC.rs] project handles all WebRTC-related state and APIs (i.e. peer connections, ICE candidates, media management, etc.).

[Rust]: https://www.rust-lang.org
[Tokio]: https://tokio.rs
[Hyper]: https://hyper.rs
[Tungstenite]: https://github.com/snapview/tungstenite-rs
[Serde]: https://serde.rs
[`serde_json`]: https://github.com/serde-rs/json
[WebRTC.rs]: https://webrtc.rs

Logging verbosity may be customized with the `RUST_LOG` environment variable. See the [`env_logger`] crate's documentation for example usage.

[`env_logger`]: https://docs.rs/env_logger

In Bash, we run the following commands to build and run the signaling server:

```bash
# Set up logging level
export RUST_LOG=warn

# Starts the signaling server at `0.0.0.0:3000`
cargo run --release
```
