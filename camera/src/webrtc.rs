use crate::signaling;
use anyhow::{anyhow, bail, Context};
use async_std::task;
use futures::FutureExt;
use gstreamer::prelude::*;
use gstreamer::{ElementFlags, MessageType};
use signaling_types::protocol::{PeerMessage, ServerMessage};
use signaling_types::rooms::Message as RoomMessage;

#[allow(unused)]
const WEBRTC_PIPELINE: &str = "v4l2src device=/dev/video2 name=src ! vp8enc ! rtpvp8pay pt=96 \
     ! webrtcbin. webrtcbin name=webrtcbin stun-server=stun://stun.l.google.com:19302";

#[allow(unused)]
const WEBRTC_PIPELINE_TEST: &str =
    "videotestsrc pattern=ball is-live=true ! vp8enc ! rtpvp8pay pt=96 \
    ! webrtcbin. webrtcbin name=webrtcbin stun-server=stun://stun.l.google.com:19302";

#[allow(unused)]
const WEBRTC_PIPELINE_DESKTOP: &str =
    "ximagesrc ! videorate ! videoscale ! video/x-raw,width=1920,height=1080,framerate=5/1 \
    ! videoconvert ! vp8enc ! rtpvp8pay pt=96 ! webrtcbin. webrtcbin name=webrtcbin";

#[allow(unused)]
const WEBRTC_PIPELINE_FANCY: &str = "v4l2src device=/dev/video0 ! videorate ! videoscale \
    ! video/x-raw,width=640,height=360,framerate=5/1 ! videoconvert ! queue max-size-buffers=1 \
    ! x264enc bitrate=600 speed-preset=ultrafast tune=zerolatency key-int-max=15 \
    ! video/x-h264,profile=constrained-baseline ! queue max-size-time=100000000 ! h264parse \
    ! rtph264pay config-interval=-1 name=payloader aggregate-mode=zero-latency \
    ! application/x-rtp,media=video,encoding-name=H264,payload=96 ! webrtcbin. \
    webrtcbin name=webrtcbin stun-server=stun://stun.l.google.com:19302 ";

#[allow(unused)]
const WEBRTC_PIPELINE_LIBCAMERA: &str = "libcamerasrc ! video/x-raw,width=1600,height=1200 \
     ! videoconvert ! videoflip motion=vertical-flip ! vp8enc ! rtpvp8pay pt=96 \
     ! webrtcbin. webrtcbin name=webrtcbin stun-server=stun://stun.l.google.com:19302";

pub async fn entry(mut websocket: crate::signaling::WebSocket) -> anyhow::Result<()> {
    let pipeline = gstreamer::parse_launch(WEBRTC_PIPELINE_FANCY)
        .unwrap()
        .downcast::<gstreamer::Pipeline>()
        .map_err(|_| anyhow!("Failed to parse gstreamer pipeline"))?;

    let webrtcbin = pipeline
        .by_name("webrtcbin")
        .context("Failed to get element by name")?
        .dynamic_cast::<gstreamer::Element>()
        .map_err(|_| anyhow!("Failed to downcast the pipeline to the webrtc bin"))?;

    webrtcbin.set_element_flags(ElementFlags::SINK);
    pipeline.set_state(gstreamer::State::Ready)?;
    let handler = crate::florian::attach(webrtcbin)?;

    loop {
        futures::select! {
            local = handler.channel().recv().fuse() => {
                pipeline.set_state(gstreamer::State::Playing)?;
                signaling::write_to_ws(&mut websocket, &PeerMessage::Signal(local.expect("signal not there"))).await?;
            }
            remote = signaling::read_from_ws(&mut websocket).fuse() => {
                match remote {
                    Ok(ServerMessage::Room(room_msg)) => match room_msg {
                        RoomMessage::Signal { signal, .. } => { handler.process(signal); },
                        RoomMessage::Join { .. } => {
                            pipeline.set_state(gstreamer::State::Playing)?;
                            log::info!("Set pipeline to play, starting negotiation");
                        },
                        RoomMessage::Leave { .. } => {
                            pipeline.set_state(gstreamer::State::Paused)?;
                            pipeline.set_state(gstreamer::State::Ready)?;
                            pipeline.set_state(gstreamer::State::Null)?;
                            log::info!("Set pipeline to pause, stopped stream, restarting..");
                            return Ok(())
                        }
                    }
                    Ok(msg) => bail!("Unexpected message {:#?}", msg),
                    Err(e) => bail!("Websocket error {:#?}", e)
                }
            }
            _ = task::sleep(std::time::Duration::from_millis(2000)).fuse() => {

               handler.bin().emit_by_name("get-stats", &[
                    &None::<gstreamer::Pad>,
                    &gstreamer::Promise::with_change_func(move |reply| {
                        log::debug!("status:");
                        let reply = reply.expect("we needed stats :(").expect("same");
                        reply.iter().for_each(|(k,v)| log::debug!("\t{}: v {:#?}", k, v));
                    })
                ]).ok();
            }
        }
    }
}
