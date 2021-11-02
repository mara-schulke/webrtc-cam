use gstreamer::prelude::*;

//use anyhow::anyhow;
//use async_tungstenite::tungstenite::Message;
//use async_std::sync::*;
//use futures::prelude::*;
//use http_types::{Response, StatusCode};

//use gstreamer::{parse_bin_from_description, ElementFlags, MessageType};
//use std::io::Write;
//use std::sync::Arc;
//
//
//

const WEBRTC_TEST_PIPELINE: &str = "videotestsrc pattern=ball is-live=true ! vp8enc deadline=1 ! rtpvp8pay pt=96 \
                                    ! webrtcbin. audiotestsrc is-live=true ! opusenc ! rtpopuspay pt=97 \
                                    ! webrtcbin. webrtcbin name=webrtcbin";

const WEBRTC_PIPELINE_FANCY: &str = "webrtcbin name=webrtcbin stun-server=stun://stun.l.google.com:19302 v4l2src device=/dev/video4 ! videorate ! videoscale \
                                    ! video/x-raw,width=640,height=360,framerate=15/1 ! videoconvert ! queue max-size-buffers=1 \
                                    ! x264enc bitrate=600 speed-preset=ultrafast tune=zerolatency key-int-max=15 \
                                    ! video/x-h264,profile=constrained-baseline ! queue max-size-time=100000000 ! h264parse \
                                    ! rtph264pay config-interval=-1 name=payloader aggregate-mode=zero-latency \
                                    ! application/x-rtp,media=video,encoding-name=H264,payload=96 ! webrtcbin. ";

const WEBRTC_PIPELINE: &str =
    "videotestsrc name=src ! vp8enc ! rtpvp8pay pt=96 ! webrtcbin name=srcrtcbin";

// ! webrtcbin. autoaudiosrc is-live=1 ! queue max-size-buffers=1 leaky=downstream \
// ! audioconvert ! audioresample ! opusenc ! rtpopuspay pt=97 ! webrtcbin. ";

//const TEST_PIPELINE: &str = "videotestsrc pattern=ball is-live=1 ! video/x-raw,width=320,height=240,framerate=1/1 ! jpegenc ! appsink name=appsink drop=true max-buffers=1";

#[async_std::main]
async fn main() -> anyhow::Result<()> {
    gstreamer::init().unwrap();

    let pipeline = gstreamer::parse_launch(WEBRTC_PIPELINE_FANCY)
        .unwrap()
        .downcast::<gstreamer::Pipeline>()
        .unwrap();

    pipeline.set_state(gstreamer::State::Playing)?;

    let webrtcbin = pipeline.by_name("webrtcbin").unwrap();
    let webrtcbin = webrtcbin.dynamic_cast::<gstreamer::Element>().unwrap();
    webrtcbin.set_property_from_str("bundle-policy", "max-bundle");

    dbg!(webrtcbin);

    Ok(())
}
