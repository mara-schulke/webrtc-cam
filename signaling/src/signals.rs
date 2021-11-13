use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IceOrSdp {
    Ice(Ice),
    Sdp(Sdp),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ice {
    candidate: String,
    #[serde(rename = "sdpMLineIndex")]
    line_index: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sdp {
    r#type: SdpType,
    sdp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SdpType {
    Offer,
    Answer,
}