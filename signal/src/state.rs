use hyper::{Body, Request, Response, StatusCode};
use std::sync::Arc;
use tokio::sync::broadcast;
use webrtc::{
    api::API,
    ice_transport::ice_candidate::RTCIceCandidate,
    peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection},
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

struct HostCreated {
    peer: RTCPeerConnection,
    answer: RTCSessionDescription,
    ice_rx: flume::Receiver<RTCIceCandidate>,
    track_rx: broadcast::Receiver<Arc<TrackLocalStaticRTP>>,
}

pub struct State {
    /// WebRTC global API manager. Used for creating new peer connections.
    api: API,
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

        Ok(Self { api })
    }

    pub async fn on_request(&self, req: Request<Body>) -> Result<Response<Body>, StatusCode> {
        use hyper::{header, upgrade, Method};

        if *req.method() != Method::GET {
            return Err(StatusCode::METHOD_NOT_ALLOWED);
        }

        let path_and_query = req.uri().path_and_query().ok_or(StatusCode::BAD_REQUEST)?;
        let is_host = match path_and_query.path() {
            "/ws/client" => false,
            "/ws/host" => true,
            _ => return Err(StatusCode::NOT_FOUND),
        };

        // TODO: Check if a host with the same name already exists
        let query = path_and_query.query();

        // Perform WebSocket handshake
        let (accept, offer) = super::ws::validate_headers(req.headers()).ok_or(StatusCode::BAD_REQUEST)?;
        let ws_accept = HeaderValue::from_str(&accept).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let upgrade_future = upgrade::on(req);

        use tokio_tungstenite::{
            tungstenite::protocol::{Message, Role},
            WebSocketStream,
        };
        if is_host {
            let HostCreated { peer, answer, ice_rx, track_rx } =
                self.spawn_new_host(offer).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            tokio::spawn(async move {
                use futures_util::{
                    future::{select, Either},
                    SinkExt, TryStreamExt,
                };

                let io = upgrade_future.await.expect("failed to upgrade WebSocket");
                let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;

                let json = serde_json::to_string(&answer).expect("answer serialization failed");
                ws.send(Message::Text(json)).await.expect("cannot send answer");

                loop {
                    match select(ice_rx.recv_async(), ws.try_next()).await {
                        Either::Left((ice_result, _)) => {
                            let ice = match ice_result {
                                Ok(ice) => ice,
                                _ => break,
                            };
                            let json = serde_json::to_string(&ice).expect("ICE serialization failed");
                            ws.send(Message::Text(json)).await.expect("cannot send answer");
                        }
                        Either::Right((msg_result, _)) => {
                            let msg = match msg_result.expect("stream exception") {
                                Some(msg) => msg,
                                _ => break,
                            };
                            let txt = match msg {
                                Message::Text(txt) => txt,
                                Message::Close(_) => break,
                                _ => continue,
                            };
                            let ice = serde_json::from_str(&txt).expect("ICE deserialization failed");
                            peer.add_ice_candidate(ice).await.expect("cannot add ICE candidate");
                        }
                    }
                }
            });
        } else {
            todo!()
        }

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

    async fn spawn_new_host(&self, offer: RTCSessionDescription) -> webrtc::error::Result<HostCreated> {
        let peer = self.api.new_peer_connection(Default::default()).await?;
        peer.set_remote_description(offer).await?;
        peer.add_transceiver_from_kind(webrtc::rtp_transceiver::rtp_codec::RTPCodecType::Video, &[]).await?;
        let answer = peer.create_answer(None).await?;

        let (ice_tx, ice_rx) = flume::unbounded();
        peer.on_ice_candidate(Box::new(move |maybe_ice| {
            let tx = ice_tx.clone();
            if let Some(ice) = maybe_ice {
                tx.send(ice).expect("ICE receiver closed");
            }

            Box::pin(core::future::ready(()))
        }))
        .await;

        let (track_tx, track_rx) = broadcast::channel(8);
        peer.on_track(Box::new(move |maybe_track, _| {
            let tx = track_tx.clone();
            Box::pin(async move {
                let remote_track = match maybe_track {
                    Some(track) => track,
                    _ => return,
                };

                use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecParameters;
                let RTCRtpCodecParameters { capability, .. } = remote_track.codec().await;
                let local_track =
                    Arc::new(TrackLocalStaticRTP::new(capability, "host-video".to_owned(), "host-stream".to_owned()));
                tx.send(local_track.clone()).unwrap();

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

        Ok(HostCreated { peer, answer, ice_rx, track_rx })
    }
}
