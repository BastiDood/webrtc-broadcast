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
        log::info!("WebSocket upgrade requested...");
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
            log::info!("Upgraded to WebSocket.");

            let mut ws = WebSocketStream::from_raw_socket(io, Role::Server, None).await;
            let peer = self.api.new_peer_connection(Default::default()).await.unwrap();
            let arc_peer = Arc::new(peer);

            let (ice_tx, ice_rx) = flume::unbounded();
            arc_peer
                .on_ice_candidate(Box::new(move |maybe_ice| {
                    log::info!("icecandidate event triggered!");
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
                log::info!("Set host state to Pending.");

                // Listen for new tracks
                let weak_peer_outer = Arc::downgrade(&arc_peer);
                let (track_tx, track_rx) = flume::bounded(1);
                arc_peer
                    .on_track(Box::new(move |maybe_track, _| {
                        log::info!("track event triggered!");
                        let tx = track_tx.clone();
                        let weak_peer = weak_peer_outer.clone();
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

                            let media_ssrc = remote_track.ssrc();
                            tokio::spawn(async move {
                                let mut interval = tokio::time::interval(core::time::Duration::from_secs(3));
                                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                                loop {
                                    use webrtc::{
                                        rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
                                        track::track_local::TrackLocalWriter,
                                    };
                                    let packet = tokio::select! {
                                        _ = interval.tick() => {
                                            if let Some(peer) = weak_peer.upgrade() {
                                                peer.write_rtcp(&[Box::new(PictureLossIndication { sender_ssrc: 0, media_ssrc })])
                                                    .await
                                                    .expect("cannot send PLI packet");
                                                log::trace!("Sent PLI packet to remote.");
                                                continue;
                                            } else {
                                                break;
                                            }
                                        }
                                        Ok((packet, _)) = remote_track.read_rtp() => packet,
                                        else => unimplemented!(),
                                    };

                                    log::trace!("Received remote track packet.");
                                    match local_track.write_rtp(&packet).await {
                                        Ok(_) => log::trace!("Written packet to local track."),
                                        Err(webrtc::Error::ErrClosedPipe) => break,
                                        _ => unimplemented!(),
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
                log::info!("Received offer from remote host.");

                // Send reply to the remote
                arc_peer.set_remote_description(offer).await.unwrap();
                let answer = arc_peer.create_answer(None).await.unwrap();
                let desc = serde_json::to_string(&answer).unwrap();
                ws.send(Message::Text(desc)).await.expect("cannot send answer");
                arc_peer.set_local_description(answer).await.unwrap();
                log::info!("Sent answer to the remote host.");

                Some(track_rx)
            } else {
                // Check if a host exists
                let local_track = self.host.read().await.get_ready().expect("no host found").clone();
                arc_peer.add_track(local_track).await.expect("cannot add local track");
                log::info!("Cloned local track to remote client.");

                // Send remote an offer
                let offer = arc_peer.create_offer(None).await.unwrap();
                let desc = serde_json::to_string(&offer).unwrap();
                ws.send(Message::Text(desc)).await.expect("cannot send offer");
                log::info!("Sent offer to remote client.");

                // Wait for remote's answer
                let desc =
                    ws.try_next().await.expect("cannot retrieve offer").expect("stream closed").into_text().unwrap();
                let answer = serde_json::from_str(&desc).expect("cannot deserialize offer");
                log::info!("Sent offer to remote client.");

                arc_peer.set_local_description(offer).await.unwrap();
                arc_peer.set_remote_description(answer).await.unwrap();
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
                        log::info!("Sent ICE candidate to remote.");
                    }
                    msg_result = ws.try_next() => {
                        let ice = match msg_result.expect("WebSocket stream error").expect("WebSocket stream closed") {
                            Message::Text(txt) => serde_json::from_str(&txt).expect("ICE deserialization failed"),
                            Message::Close(_) => break,
                            _ => continue,
                        };
                        arc_peer.add_ice_candidate(ice).await.expect("cannot add ICE candidate");
                        log::info!("Received ICE candidate from remote.");
                    }
                    track_result = track_fut => {
                        let track = track_result.expect("track channel closed");
                        self.host.write().await.set_ready(track);
                        log::info!("Received track from remote host.");
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
