use serde::{Deserialize, Serialize};
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Signal {
    Sdp(RTCSessionDescription),
    Ice(RTCIceCandidateInit),
}
