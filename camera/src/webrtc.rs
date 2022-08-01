use crate::signaling;
use crate::signals;
use crate::signals::Event;
use anyhow::anyhow;
use async_std::channel::{unbounded, Receiver, Sender};
use async_std::stream::StreamExt;
use async_std::sync::Arc;
use async_std::task;
use futures::FutureExt;
use std::io::Cursor;
use std::time::Duration;
use std::time::SystemTime;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_connection_state::RTCIceConnectionState;
use webrtc::ice_transport::ice_credential_type::RTCIceCredentialType;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::media::io::h264_reader::H264Reader;
use webrtc::media::Sample;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::RTCRtpTransceiverInit;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

pub async fn entry(mut websocket: crate::signaling::WebSocket) -> anyhow::Result<()> {
    let config = RTCConfiguration {
        ice_servers: vec![
            RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            },
            RTCIceServer {
                urls: vec!["turn:openrelay.metered.ca:80".to_owned()],
                username: "openrelayproject".to_owned(),
                credential: "openrelayproject".to_owned(),
                credential_type: RTCIceCredentialType::Password,
            },
            RTCIceServer {
                urls: vec!["turn:openrelay.metered.ca:443".to_owned()],
                username: "openrelayproject".to_owned(),
                credential: "openrelayproject".to_owned(),
                credential_type: RTCIceCredentialType::Password,
            },
            RTCIceServer {
                urls: vec!["turn:openrelay.metered.ca:443?transport=tcp".to_owned()],
                username: "openrelayproject".to_owned(),
                credential: "openrelayproject".to_owned(),
                credential_type: RTCIceCredentialType::Password,
            },
        ],
        ..Default::default()
    };

    let mut media = MediaEngine::default();
    media.register_default_codecs()?;

    let registry = register_default_interceptors(Registry::new(), &mut media)?;

    let api = APIBuilder::new()
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .build();

    let events = Events::new(api.new_peer_connection(config).await?).await;

    events
        .peer
        .add_transceiver_from_kind(
            RTPCodecType::Audio,
            &vec![RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendrecv,
                send_encodings: vec![],
            }],
        )
        .await?;

    events
        .peer
        .add_transceiver_from_kind(
            RTPCodecType::Video,
            &vec![RTCRtpTransceiverInit {
                direction: RTCRtpTransceiverDirection::Sendonly,
                send_encodings: vec![],
            }],
        )
        .await?;

    let mut interval = async_std::stream::interval(Duration::from_millis(10000));

    let res = loop {
        futures::select! {
            out = events.out.recv().fuse() => {
                signaling::write_to_ws(&mut websocket, &out.expect("signal not there")).await?;
            }
            remote = signaling::read_from_ws(&mut websocket).fuse() => {
                match remote {
                    Ok(event) => match event {
                        Event::Start => {
                            log::warn!("received start");
                        }
                        Event::Stop => {
                            break Ok(())
                        }
                        Event::Signal(signal) => {
                            dbg!(&signal);
                            events.inc.send(signal).await.ok();
                        }
                    },
                    Err(e) => break Err(anyhow!("Websocket error {:#?}", e))
                }
            }
            state = events.peer_state.recv().fuse() => {
                match state {
                    Ok(RTCPeerConnectionState::Connected) => {
                        log::info!("connected");
                        task::spawn(video_stream(events.peer.clone()));
                        task::spawn(audio_stream(events.peer.clone()));
                    }
                    Ok(RTCPeerConnectionState::Closed | RTCPeerConnectionState::Disconnected) => {
                        break Ok(());
                    }
                    Ok(RTCPeerConnectionState::Failed) => {
                        break Err(anyhow!("connection failed"))
                    }
                    _ => { }
                }
            }
            state = events.ice_state.recv().fuse() => {
                match state {
                    Ok(RTCIceConnectionState::Failed) => {
                        break Err(anyhow!("connection failed"))
                    }
                    _ => {}
                }
            }
            _ = interval.next().fuse() => {
                log::info!("connection state: {:?}", events.peer.connection_state());
                log::info!("signaling state: {:?}", events.peer.signaling_state());
                log::info!("ice connection state: {:?}", events.peer.ice_connection_state());
                log::info!("ice gathering state: {:?}", events.peer.ice_gathering_state());

                if let RTCPeerConnectionState::New = events.peer.connection_state() {
                    break Err(anyhow!("retry, client did not answer in time"))
                }
            }
        }
    };

    events.peer.close().await?;

    res
}

async fn video_stream(peer: Arc<RTCPeerConnection>) -> anyhow::Result<()> {
    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "livy-alive".to_owned(),
    ));

    let rtp_sender = peer
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    log::info!("added video track");

    task::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        anyhow::Result::<()>::Ok(())
    });

    let file = include_bytes!("../rick.h264");
    let mut h264 = H264Reader::new(Cursor::new(file));

    log::info!("playing video track");

    let mut frames = async_std::stream::interval(Duration::from_millis(40));

    while let Some(_) = frames.next().await {
        let data = match h264.next_nal() {
            Ok(nal) => nal.data.freeze(),
            Err(e) => {
                log::error!("error while reading nal: {:?}", e);
                break;
            }
        };

        video_track
            .write_sample(&Sample {
                data,
                duration: Duration::from_secs(1),
                timestamp: SystemTime::now(),
                ..Default::default()
            })
            .await?;
    }

    peer.close().await?;

    Ok(())
}

async fn audio_stream(peer: Arc<RTCPeerConnection>) -> anyhow::Result<()> {
    use webrtc::media::io::ogg_reader::OggReader;

    const OGG_PAGE_DURATION: Duration = Duration::from_millis(20);

    let audio_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_OPUS.to_owned(),
            clock_rate: 48000,
            ..Default::default()
        },
        "audio".to_owned(),
        "livy-alive".to_owned(),
    ));

    let rtp_sender = peer
        .add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await?;

    log::info!("added audio track");

    task::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        anyhow::Result::<()>::Ok(())
    });

    let file = include_bytes!("../rick.ogg");
    let (mut ogg, _) = OggReader::new(Cursor::new(file), true)?;

    log::info!("playing audio track");

    let mut frames = async_std::stream::interval(OGG_PAGE_DURATION);

    let mut last_granule: u64 = 0;

    while let Some(_) = frames.next().await {
        let (page, header) = match ogg.parse_next_page() {
            Ok(page_and_header) => page_and_header,
            Err(err) => {
                log::error!("audio stream ended: {:?}", err);
                break;
            }
        };

        let sample_count = header
            .granule_position
            .checked_sub(last_granule)
            .unwrap_or(0);
        last_granule = header.granule_position;

        audio_track
            .write_sample(&Sample {
                data: page.freeze(),
                duration: Duration::from_millis(sample_count * 1000 / 48000),
                timestamp: SystemTime::now(),
                ..Default::default()
            })
            .await?;
    }

    peer.close().await?;

    Ok(())
}

#[derive(Clone)]
struct Events {
    pub peer: Arc<RTCPeerConnection>,
    pub inc: Sender<signals::Signal>,
    pub out: Receiver<signals::Signal>,
    pub peer_state: Receiver<RTCPeerConnectionState>,
    pub ice_state: Receiver<RTCIceConnectionState>,
}

impl Events {
    async fn new(con: RTCPeerConnection) -> Self {
        let peer = Arc::new(con);
        let (inc_tx, inc_rx) = unbounded::<signals::Signal>();
        let (out_tx, out_rx) = unbounded::<signals::Signal>();
        let (peer_state_tx, peer_state_rx) = unbounded();
        let (ice_state_tx, ice_state_rx) = unbounded();

        let tx = out_tx.clone();
        peer.on_ice_candidate(Box::new(move |ice: Option<RTCIceCandidate>| {
            let tx = tx.clone();
            Box::pin(async move {
                if let Some(ice) = ice {
                    let signal = ice
                        .to_json()
                        .await
                        .expect("failed to serialize ice candidate");

                    let signal = signals::Signal::Ice(signal);

                    tx.send(signal).await.ok();
                }
            })
        }))
        .await;

        let inner = Arc::downgrade(&peer);
        let tx = out_tx.clone();
        peer.on_negotiation_needed(Box::new(move || {
            let tx = tx.clone();
            let peer: Arc<RTCPeerConnection> = inner.upgrade().expect("unable to upgrade");

            log::info!("on negotiation needed");

            Box::pin(async move {
                let offer = peer
                    .create_offer(None)
                    .await
                    .expect("unable to offer sdp session");

                let signal = signals::Signal::Sdp(offer.clone());

                peer.set_local_description(offer).await.unwrap();

                tx.send(signal).await.ok();
            })
        }))
        .await;

        let tx = peer_state_tx.clone();
        peer.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            let tx = tx.clone();
            Box::pin(async move {
                tx.send(s).await.ok();
            })
        }))
        .await;

        let tx = ice_state_tx.clone();
        peer.on_ice_connection_state_change(Box::new(move |s: RTCIceConnectionState| {
            let tx = tx.clone();
            Box::pin(async move {
                tx.send(s).await.ok();
            })
        }))
        .await;

        {
            let peer = peer.clone();
            let rx = inc_rx.clone();
            let tx = out_tx.clone();
            task::spawn(async move {
                while let Ok(inc) = rx.recv().await {
                    match inc {
                        signals::Signal::Ice(ice) => {
                            peer.add_ice_candidate(ice).await.ok();
                        }
                        signals::Signal::Sdp(sdp) => {
                            match sdp.sdp_type {
                                RTCSdpType::Offer => {
                                    peer.set_remote_description(sdp).await.unwrap();
                                    let answer = peer.create_answer(None).await.unwrap();
                                    peer.set_local_description(answer.clone()).await.unwrap();
                                    let signal = signals::Signal::Sdp(answer);
                                    tx.send(signal).await.ok();
                                }
                                RTCSdpType::Answer => {
                                    peer.set_remote_description(sdp).await.unwrap();
                                }
                                sdp_type => {
                                    log::warn!("unable to handle sdp type: {:?}", sdp_type);
                                }
                            }

                            log::info!("recv sdp");
                        }
                    }
                }
            });
        }

        Self {
            peer,
            inc: inc_tx,
            out: out_rx,
            peer_state: peer_state_rx,
            ice_state: ice_state_rx,
        }
    }
}
