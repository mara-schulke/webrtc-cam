use gstreamer::prelude::*;

#[allow(unused)]
const WEBRTC_PIPELINE: &str =
    "videotestsrc name=src ! vp8enc ! rtpvp8pay pt=96 ! webrtcbin name=webrtcbin";

#[allow(unused)]
const WEBRTC_PIPELINE_TEST: &str = "videotestsrc pattern=ball is-live=true ! vp8enc deadline=1 ! rtpvp8pay pt=96 \
                                    ! webrtcbin. audiotestsrc is-live=true ! opusenc ! rtpopuspay pt=97 \
                                    ! webrtcbin. webrtcbin name=webrtcbin";

#[allow(unused)]
const WEBRTC_PIPELINE_FANCY: &str = "webrtcbin name=webrtcbin stun-server=stun://stun.l.google.com:19302 v4l2src device=/dev/video4 ! videorate ! videoscale \
                                    ! video/x-raw,width=640,height=360,framerate=15/1 ! videoconvert ! queue max-size-buffers=1 \
                                    ! x264enc bitrate=600 speed-preset=ultrafast tune=zerolatency key-int-max=15 \
                                    ! video/x-h264,profile=constrained-baseline ! queue max-size-time=100000000 ! h264parse \
                                    ! rtph264pay config-interval=-1 name=payloader aggregate-mode=zero-latency \
                                    ! application/x-rtp,media=video,encoding-name=H264,payload=96 ! webrtcbin. ";


pub async fn entry(websocket: crate::signaling::WebSocket) -> anyhow::Result<()> {
    loop {
        log::info!("Webrtc thread :)");
        async_std::task::sleep(std::time::Duration::from_millis(200)).await;
    }

    let pipeline = gstreamer::parse_launch(WEBRTC_PIPELINE_FANCY)
        .unwrap()
        .downcast::<gstreamer::Pipeline>()
        .unwrap();

    pipeline.set_state(gstreamer::State::Playing)?;

    let webrtcbin = pipeline.by_name("webrtcbin").unwrap().dynamic_cast::<gstreamer::Element>().unwrap();
    webrtcbin.set_property_from_str("bundle-policy", "max-bundle");

    pipeline.call_async(|pipeline| {
        log::info!("Playing");
        pipeline
            .set_state(gstreamer::State::Playing)
            .expect("Couldn't set pipeline to Playing");
    });
}