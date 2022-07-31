use hyper::{Body, Request, Response, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use webrtc::{api::API, track::track_local::track_local_static_rtp::TrackLocalStaticRTP};

#[derive(Default)]
enum Host {
    #[default]
    None,
    Pending,
    Ready(Arc<TrackLocalStaticRTP>),
}

impl Host {
    fn get_ready(&self) -> Option<&Arc<TrackLocalStaticRTP>> {
        if let Self::Ready(track) = self {
            Some(track)
        } else {
            None
        }
    }

    fn set_pending(&mut self) {
        *self = match self {
            Self::None => Self::Pending,
            Self::Pending => todo!(),
            _ => unreachable!(),
        };
    }

    fn set_ready(&mut self, track: Arc<TrackLocalStaticRTP>) {
        if let Self::Pending = self {
            *self = Self::Ready(track);
        } else {
            unreachable!();
        }
    }
}

pub struct State {
    /// WebRTC global API manager. Used for creating new peer connections.
    api: API,
    /// Handle to the current host of the stream.
    host: RwLock<Host>,
}

impl State {
    pub fn try_new() -> webrtc::error::Result<Self> {
        use webrtc::{
            api::{interceptor_registry::register_default_interceptors, media_engine::MediaEngine, APIBuilder},
            interceptor::registry::Registry,
        };

        let mut media = MediaEngine::default();
        media.register_default_codecs()?;
        let registry = register_default_interceptors(Registry::new(), &mut media)?;
        let api = APIBuilder::new().with_media_engine(media).with_interceptor_registry(registry).build();

        Ok(Self { api, host: RwLock::default() })
    }

    pub fn on_request(self: Arc<Self>, req: Request<Body>) -> Result<Response<Body>, StatusCode> {
        use hyper::{header, upgrade, Method};

        if *req.method() != Method::GET {
            return Err(StatusCode::METHOD_NOT_ALLOWED);
        }

        let is_host = match req.uri().path() {
            "/ws/client" => false,
            "/ws/host" => true,
            _ => return Err(StatusCode::NOT_FOUND),
        };

        // Perform WebSocket handshake
        let accept = super::ws::validate_headers(req.headers()).ok_or(StatusCode::BAD_REQUEST)?;
        let ws_accept = HeaderValue::from_str(&accept).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let upgrade_future = upgrade::on(req);

        tokio::spawn(async move {
            use futures_util::{SinkExt, TryStreamExt};
            use tokio_tungstenite::{
                tungstenite::protocol::{Message, Role},
                WebSocketStream,
            };

            let io = upgrade_future.await.expect("failed to upgrade WebSocket");
            let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
            let peer = self.api.new_peer_connection(Default::default()).await.unwrap();

            let (ice_tx, ice_rx) = flume::unbounded();
            peer.on_ice_candidate(Box::new(move |maybe_ice| {
                let tx = ice_tx.clone();
                if let Some(ice) = maybe_ice {
                    let _ = tx.send(ice);
                }
                Box::pin(core::future::ready(()))
            }))
            .await;

            let maybe_track = if is_host {
                // Mark this host as ready to receive
                self.host.write().await.set_pending();

                // Listen for new tracks
                let (track_tx, track_rx) = flume::bounded(1);
                peer.on_track(Box::new(move |maybe_track, _| {
                    let tx = track_tx.clone();
                    Box::pin(async move {
                        let remote_track = match maybe_track {
                            Some(track) => track,
                            _ => return,
                        };

                        let local_track = Arc::new(TrackLocalStaticRTP::new(
                            remote_track.codec().await.capability,
                            "host-video".to_owned(),
                            "host-stream".to_owned(),
                        ));
                        tx.send_async(local_track.clone()).await.expect("track receiver closed");

                        tokio::spawn(async move {
                            use webrtc::track::track_local::TrackLocalWriter;
                            while let Ok((packet, _)) = remote_track.read_rtp().await {
                                let err = match local_track.write_rtp(&packet).await {
                                    Err(err) => err,
                                    _ => continue,
                                };
                                if let webrtc::Error::ErrClosedPipe = err {
                                    break;
                                } else {
                                    unimplemented!();
                                }
                            }
                        });
                    })
                }))
                .await;

                // Wait for the remote to send an offer first
                let msg =
                    ws.try_next().await.expect("cannot retrieve offer").expect("stream closed").into_text().unwrap();
                let offer = serde_json::from_str(&msg).expect("cannot deserialize offer");

                // Send reply to the remote
                peer.set_remote_description(offer).await.unwrap();
                let answer = peer.create_answer(None).await.unwrap();
                let desc = serde_json::to_string(&answer).unwrap();
                ws.send(Message::Text(desc)).await.expect("cannot send answer");
                peer.set_local_description(answer).await.unwrap();

                Some(track_rx)
            } else {
                // Check if a host exists
                let host = self.host.read().await;
                let local_track = host.get_ready().expect("no host found");
                peer.add_track(local_track.clone()).await.expect("cannot add local track");

                // Send remote an offer
                let offer = peer.create_offer(None).await.unwrap();
                let desc = serde_json::to_string(&offer).unwrap();
                ws.send(Message::Text(desc)).await.expect("cannot send offer");

                // Wait for remote's answer
                let desc =
                    ws.try_next().await.expect("cannot retrieve offer").expect("stream closed").into_text().unwrap();
                let answer = serde_json::from_str(&desc).expect("cannot deserialize offer");

                peer.set_local_description(offer).await.unwrap();
                peer.set_remote_description(answer).await.unwrap();
                None
            };

            loop {
                use futures_util::FutureExt;
                let track_fut = if let Some(ref track) = maybe_track {
                    track.recv_async().left_future()
                } else {
                    core::future::pending().right_future()
                };

                tokio::select! {
                    biased;
                    ice_result = ice_rx.recv_async() => {
                        let ice = ice_result.expect("ICE sender closed");
                        let json = serde_json::to_string(&ice).expect("ICE serialization failed");
                        ws.send(Message::Text(json)).await.expect("cannot send ICE candidate");
                    }
                    msg_result = ws.try_next() => {
                        let ice = match msg_result.expect("WebSocket stream error").expect("WebSocket stream closed") {
                            Message::Text(txt) => serde_json::from_str(&txt).expect("ICE deserialization failed"),
                            Message::Close(_) => break,
                            _ => continue,
                        };
                        peer.add_ice_candidate(ice).await.expect("cannot add ICE candidate");
                    }
                    track_result = track_fut => {
                        let track = track_result.expect("track channel closed");
                        self.host.write().await.set_ready(track);
                    }
                }
            }
        });

        // Announce switching protocols to the client
        use hyper::http::HeaderValue;
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;

        let headers = response.headers_mut();
        headers.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
        headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        headers.insert(header::SEC_WEBSOCKET_ACCEPT, ws_accept);

        Ok(response)
    }
}
