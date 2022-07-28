use hyper::{Body, Request, Response, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use webrtc::{
    api::API,
    ice_transport::ice_candidate::RTCIceCandidate,
    peer_connection::{sdp::session_description::RTCSessionDescription, RTCPeerConnection},
    rtp_transceiver::rtp_codec::RTPCodecType,
    track::track_local::track_local_static_rtp::TrackLocalStaticRTP,
};

#[derive(Default)]
enum Host {
    #[default]
    None,
    Pending,
    Ready(Arc<TrackLocalStaticRTP>),
}

impl Host {
    fn get_ready(&self) -> Option<Arc<TrackLocalStaticRTP>> {
        if let Self::Ready(track) = self {
            Some(track.clone())
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

struct HostCreated {
    peer: RTCPeerConnection,
    answer: String,
    ice_rx: flume::Receiver<RTCIceCandidate>,
    track_rx: flume::Receiver<Arc<TrackLocalStaticRTP>>,
}

struct ClientCreated {
    peer: RTCPeerConnection,
    answer: String,
    ice_rx: flume::Receiver<RTCIceCandidate>,
}

pub struct State {
    /// WebRTC global API manager. Used for creating new peer connections.
    api: API,
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

    pub async fn on_request(self: Arc<Self>, req: Request<Body>) -> Result<Response<Body>, StatusCode> {
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
        let (accept, offer) = super::ws::validate_headers(req.headers()).ok_or(StatusCode::BAD_REQUEST)?;
        let ws_accept = HeaderValue::from_str(&accept).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let upgrade_future = upgrade::on(req);

        use tokio_tungstenite::{
            tungstenite::protocol::{Message, Role},
            WebSocketStream,
        };

        let (peer, answer, ice_rx, maybe_host) = if is_host {
            let HostCreated { peer, answer, ice_rx, track_rx } =
                self.spawn_new_host(offer).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            self.host.write().await.set_pending();
            let this = self.clone();
            (peer, answer, ice_rx, Some((track_rx, this)))
        } else {
            let ClientCreated { peer, answer, ice_rx } = self
                .spawn_new_client(offer)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            (peer, answer, ice_rx, None)
        };

        tokio::spawn(async move {
            use futures_util::{
                future::{select, Either},
                SinkExt, TryStreamExt,
            };

            let io = upgrade_future.await.expect("failed to upgrade WebSocket");
            let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
            ws.send(Message::Text(answer)).await.expect("cannot send answer");

            // Wait for single track from host
            if let Some((track_rx, this)) = maybe_host {
                let track = track_rx.recv_async().await.expect("track sender closed");
                this.host.write().await.set_ready(track);
            }

            loop {
                match select(ice_rx.recv_async(), ws.try_next()).await {
                    Either::Left((ice_result, _)) => {
                        let ice = match ice_result {
                            Ok(ice) => ice,
                            _ => break,
                        };
                        let json = serde_json::to_string(&ice).expect("ICE serialization failed");
                        ws.send(Message::Text(json)).await.expect("cannot send ICE candidate");
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
        peer.add_transceiver_from_kind(RTPCodecType::Video, &[]).await?;

        let desc = peer.create_answer(None).await?;
        let answer = serde_json::to_string(&desc).unwrap();
        peer.set_local_description(desc).await?;

        let (ice_tx, ice_rx) = flume::unbounded();
        peer.on_ice_candidate(Box::new(move |maybe_ice| {
            let tx = ice_tx.clone();
            if let Some(ice) = maybe_ice {
                tx.send(ice).expect("ICE receiver closed");
            }

            Box::pin(core::future::ready(()))
        }))
        .await;

        let (track_tx, track_rx) = flume::unbounded();
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
                tx.send(local_track.clone()).expect("track channel closed");

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

    async fn spawn_new_client(&self, offer: RTCSessionDescription) -> webrtc::error::Result<Option<ClientCreated>> {
        let local_track = match self.host.read().await.get_ready() {
            Some(track) => track,
            _ => return Ok(None),
        };

        let peer = self.api.new_peer_connection(Default::default()).await?;
        peer.set_remote_description(offer).await?;
        peer.add_track(local_track).await?;

        let desc = peer.create_answer(None).await?;
        let answer = serde_json::to_string(&desc).unwrap();
        peer.set_local_description(desc).await?;

        // TODO: relay ICE candidates
        let (ice_tx, ice_rx) = flume::unbounded();
        peer.on_ice_candidate(Box::new(move |maybe_ice| {
            let tx = ice_tx.clone();
            if let Some(ice) = maybe_ice {
                tx.send(ice).expect("ICE receiver closed");
            }

            Box::pin(core::future::ready(()))
        }))
        .await;

        Ok(Some(ClientCreated { peer, answer, ice_rx }))
    }
}
