use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fmt, io,
    net::{AddrParseError, IpAddr},
    num::{ParseFloatError, ParseIntError},
    path::Path,
    str::FromStr,
};
use thiserror::Error;
use tokio::fs;
use url::Url;

const RTMAP_REGEX: &str = r"rtpmap:([0-9]+) (.+)\/([0-9]+)\/([0-9]+)";
const RTPMAP_PAYLOAD_ID_GROUPT: usize = 1;
const RTPMAP_BITDEPTH_GROUPT: usize = 2;
const RTPMAP_SAMPLINGRATE_GROUPT: usize = 3;
const RTPMAP_CHANNELS_GROUPT: usize = 4;

const MEDIA_AND_TRANSPORT_REGEX: &str = r"(.+) ([0-9]+) (.+) ([0-9]+)";
const MEDIA_AND_TRANSPORT_MEDIA_GROUP: usize = 1;
const MEDIA_AND_TRANSPORT_PORT_GROUP: usize = 2;
const MEDIA_AND_TRANSPORT_PROTOCOL_GROUP: usize = 3;
const MEDIA_AND_TRANSPORT_PAYLOAD_ID_GROUP: usize = 4;

const CONNECTION_INFO_REGEX: &str = r"(.+) (IP[4,6]) ([0-9]+\.[0-9]+\.[0-9]+\.[0-9]+)\/([0-9]+)";
const CONNECTION_INFO_MULTICAST_GROUP: usize = 3;

const PTIME_REGEX: &str = r"ptime:(.+)";
const PTIME_GROUP: usize = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct RtpMap {
    payload_id: u16,
    bit_depth: BitDepth,
    sample_rate: u32,
    channels: u16,
}

impl FromStr for RtpMap {
    type Err = SdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(RTMAP_REGEX).expect("cannot fail");
        if let Some(caps) = re.captures(s) {
            Ok(RtpMap {
                payload_id: caps
                    .get(RTPMAP_PAYLOAD_ID_GROUPT)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
                bit_depth: caps
                    .get(RTPMAP_BITDEPTH_GROUPT)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
                sample_rate: caps
                    .get(RTPMAP_SAMPLINGRATE_GROUPT)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
                channels: caps
                    .get(RTPMAP_CHANNELS_GROUPT)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
            })
        } else {
            Err(SdpError::FormatError)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BitDepth {
    L16,
    L24,
    L32,
    FloatingPoint,
}

impl fmt::Display for BitDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BitDepth::L16 => write!(f, "L16"),
            BitDepth::L24 => write!(f, "L24"),
            BitDepth::L32 => write!(f, "L32"),
            BitDepth::FloatingPoint => write!(f, "Floating Point"),
        }
    }
}

impl BitDepth {
    pub fn bits(&self) -> u16 {
        match self {
            BitDepth::L16 => 16,
            BitDepth::L24 => 24,
            BitDepth::L32 => 32,
            BitDepth::FloatingPoint => 32,
        }
    }

    pub fn floating_point(&self) -> bool {
        match self {
            BitDepth::FloatingPoint => true,
            _ => false,
        }
    }
}

impl FromStr for BitDepth {
    type Err = SdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains("16") {
            return Ok(BitDepth::L16);
        } else if s.contains("24") {
            return Ok(BitDepth::L24);
        } else if s.contains("32") {
            return Ok(BitDepth::L32);
        } else if s.to_lowercase().contains("float") {
            return Ok(BitDepth::FloatingPoint);
        } else {
            return Err(SdpError::FormatError);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaAndTransport {
    media: Media,
    port: u16,
    protocol: String,
    payload_id: u16,
}

impl FromStr for MediaAndTransport {
    type Err = SdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(MEDIA_AND_TRANSPORT_REGEX).expect("cannot fail");
        if let Some(caps) = re.captures(s) {
            Ok(MediaAndTransport {
                media: caps
                    .get(MEDIA_AND_TRANSPORT_MEDIA_GROUP)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
                port: caps
                    .get(MEDIA_AND_TRANSPORT_PORT_GROUP)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
                protocol: caps
                    .get(MEDIA_AND_TRANSPORT_PROTOCOL_GROUP)
                    .expect("must exist in matches")
                    .as_str()
                    .to_owned(),
                payload_id: caps
                    .get(MEDIA_AND_TRANSPORT_PAYLOAD_ID_GROUP)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
            })
        } else {
            Err(SdpError::FormatError)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Media {
    Audio,
    Video,
}

impl FromStr for Media {
    type Err = SdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "audio" => Ok(Media::Audio),
            "video" => Ok(Media::Video),
            _ => Err(SdpError::FormatError),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionInfo {
    multicast_address: IpAddr,
}

impl FromStr for ConnectionInfo {
    type Err = SdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(CONNECTION_INFO_REGEX).expect("cannot fail");
        if let Some(caps) = re.captures(s) {
            Ok(ConnectionInfo {
                multicast_address: caps
                    .get(CONNECTION_INFO_MULTICAST_GROUP)
                    .expect("must exist in matches")
                    .as_str()
                    .parse()?,
            })
        } else {
            Err(SdpError::FormatError)
        }
    }
}

fn parse_packet_time(attribue: &str) -> SdpResult<f32> {
    let re = Regex::new(PTIME_REGEX).expect("cannot fail");
    if let Some(caps) = re.captures(attribue) {
        Ok(caps
            .get(PTIME_GROUP)
            .expect("must exist in matches")
            .as_str()
            .parse()?)
    } else {
        Err(SdpError::FormatError)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SdpValue {
    ProtocolVersion(u16),                            // v
    OriginatorAndSessionIdentifier(String),          // o
    SessionName(String),                             // s
    ActiveTime((usize, usize)),                      // t
    MediaNameAndTransportAddress(MediaAndTransport), // m
    SessionInfo(String),                             // i
    SessionDescription(String),                      // u
    ConnectionInformation(ConnectionInfo),           // c
    Attribute(String),                               // a
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sdp {
    pub version: u16,              // v field
    pub multicast_port: u16,       // m field
    pub multicast_address: IpAddr, // c field
    pub payload_id: u16,           // m/a(rtpmap) field
    pub packet_time: f32,          // a(ptime) field
    pub bit_depth: BitDepth,       // a(rtpmap) field
    pub sample_rate: u32,          // a(rtpmap) field
    pub channels: u16,             // a(rtpmap) field
}

pub async fn sdp_from_url(url: &Url) -> SdpResult<Sdp> {
    let sdp_content = reqwest::get(url.as_str()).await?.text().await?;
    log::debug!("SDP: \n{sdp_content}");
    sdp_from_str(&sdp_content)
}

pub async fn sdp_from_file(path: impl AsRef<Path>) -> SdpResult<Sdp> {
    let sdp_content = fs::read_to_string(path).await?;
    log::debug!("SDP: \n{sdp_content}");
    sdp_from_str(&sdp_content)
}

pub fn sdp_from_str(sdp: &str) -> SdpResult<Sdp> {
    sdp.parse()
}

fn parse_line(line: &str) -> SdpResult<Option<(&str, SdpValue)>> {
    let trim = line.trim();

    if trim.starts_with("#") || trim.is_empty() {
        return Ok(None);
    }

    let mut kv = trim.split("=");
    if let (Some(key), Some(value)) = (kv.next(), kv.next()) {
        if let Some(value) = parse_value(key, value)? {
            Ok(Some((key, value)))
        } else {
            Ok(None)
        }
    } else {
        Err(SdpError::FormatError)
    }
}

fn parse_value(key: &str, value: &str) -> SdpResult<Option<SdpValue>> {
    match key {
        "v" => {
            let version: u16 = value.parse()?;
            Ok(Some(SdpValue::ProtocolVersion(version)))
        }
        "o" => Ok(Some(SdpValue::OriginatorAndSessionIdentifier(
            value.to_owned(),
        ))),
        "s" => Ok(Some(SdpValue::SessionName(value.to_owned()))),
        "t" => {
            let mut times = value.split(" ");
            if let (Some(time_1), Some(time_2)) = (times.next(), times.next()) {
                Ok(Some(SdpValue::ActiveTime((
                    time_1.parse()?,
                    time_2.parse()?,
                ))))
            } else {
                Err(SdpError::FormatError)
            }
        }
        "m" => Ok(Some(SdpValue::MediaNameAndTransportAddress(value.parse()?))),
        "i" => Ok(Some(SdpValue::SessionInfo(value.to_owned()))),
        "u" => Ok(Some(SdpValue::SessionDescription(value.to_owned()))),
        "c" => Ok(Some(SdpValue::ConnectionInformation(value.parse()?))),
        "a" => Ok(Some(SdpValue::Attribute(value.to_owned()))),
        _ => Ok(None),
    }
}

impl FromStr for Sdp {
    type Err = SdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lines = s.split("\n");

        let mut bit_depth = None;
        let mut channels = None;
        let mut multicast_address = None;
        let mut multicast_port = None;
        let mut packet_time = None;
        let mut payload_id = None;
        let mut sample_rate = None;
        let mut version = None;

        for line in lines {
            if let Some((_, value)) = parse_line(line)? {
                match value {
                    SdpValue::ProtocolVersion(v) => version = Some(v),
                    SdpValue::OriginatorAndSessionIdentifier(_) => {}
                    SdpValue::SessionName(_) => {}
                    SdpValue::ActiveTime(_) => {}
                    SdpValue::MediaNameAndTransportAddress(m) => {
                        payload_id = Some(m.payload_id);
                        multicast_port = Some(m.port);
                    }
                    SdpValue::SessionInfo(_) => {}
                    SdpValue::SessionDescription(_) => {}
                    SdpValue::ConnectionInformation(c) => {
                        multicast_address = Some(c.multicast_address)
                    }
                    SdpValue::Attribute(a) => {
                        if let Ok(rtpmap) = a.parse::<RtpMap>() {
                            sample_rate = Some(rtpmap.sample_rate);
                            channels = Some(rtpmap.channels);
                            bit_depth = Some(rtpmap.bit_depth);
                        }
                        if let Ok(ptime) = parse_packet_time(&a) {
                            packet_time = Some(ptime);
                        }
                    }
                }
            }
        }

        if let (
            Some(bit_depth),
            Some(channels),
            Some(multicast_address),
            Some(multicast_port),
            Some(packet_time),
            Some(payload_id),
            Some(sample_rate),
            Some(version),
        ) = (
            bit_depth,
            channels,
            multicast_address,
            multicast_port,
            packet_time,
            payload_id,
            sample_rate,
            version,
        ) {
            Ok(Sdp {
                bit_depth,
                channels,
                multicast_address,
                multicast_port,
                packet_time,
                payload_id,
                sample_rate,
                version,
            })
        } else {
            Err(SdpError::FormatError)
        }
    }
}

#[derive(Error, Debug)]
pub enum SdpError {
    #[error("sdp format error")]
    FormatError,
    #[error("parse int error: {0}")]
    ParseVersionError(#[from] ParseIntError),
    #[error("parse float error: {0}")]
    ParseFloatError(#[from] ParseFloatError),
    #[error("addr parse error: {0}")]
    AddrParseError(#[from] AddrParseError),
    #[error("reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("io error: {0}")]
    IoError(#[from] io::Error),
}

pub type SdpResult<T> = Result<T, SdpError>;

#[cfg(test)]
mod test {
    use super::*;

    const SDP: &str = include_str!("../stream.sdp");

    #[test]
    fn parse_comment() {
        let line = "# hello world";
        let parsed = parse_line(line).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_empty_line() {
        let line = " ";
        let parsed = parse_line(line).unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_version() {
        let line = "v=0";
        let (key, value) = parse_line(line).unwrap().unwrap();
        assert_eq!(key, "v");
        assert_eq!(value, SdpValue::ProtocolVersion(0));
    }

    #[test]
    fn parse_name_and_transport() {
        let line = "m=audio 5004 RTP/AVP 98";
        let (key, value) = parse_line(line).unwrap().unwrap();
        assert_eq!(key, "m");
        assert_eq!(
            value,
            SdpValue::MediaNameAndTransportAddress(MediaAndTransport {
                media: Media::Audio,
                port: 5004,
                protocol: "RTP/AVP".to_owned(),
                payload_id: 98
            })
        );
    }

    #[test]
    fn parse_attribute() {
        let line = "a=rtpmap:98 L16/48000/8";
        let (key, value) = parse_line(line).unwrap().unwrap();
        assert_eq!(key, "a");
        assert_eq!(
            value,
            SdpValue::Attribute("rtpmap:98 L16/48000/8".to_owned())
        );
    }

    #[test]
    fn parse_rtpmap() {
        let line = "rtpmap:98 L16/48000/8";
        let rtp_map: RtpMap = line.parse().unwrap();
        assert_eq!(
            rtp_map,
            RtpMap {
                bit_depth: BitDepth::L16,
                channels: 8,
                payload_id: 98,
                sample_rate: 48000
            }
        );
    }

    #[test]
    fn from_url() {
        let _url = "http://10.1.255.252:5050/x-manufacturer/senders/ce187070-000a-102b-bb00-000000000000/stream.sdp";
        // TODO
    }

    #[test]
    fn from_str() {
        let sdp = sdp_from_str(SDP).unwrap();
        assert_eq!(
            sdp,
            Sdp {
                bit_depth: BitDepth::L16,
                channels: 8,
                multicast_port: 5004,
                payload_id: 98,
                version: 0,
                multicast_address: "239.0.0.1".parse().unwrap(),
                packet_time: 0.125,
                sample_rate: 48000
            }
        )
    }
}
