use std::{
    io,
    net::AddrParseError,
    num::{ParseFloatError, ParseIntError},
};

use cpal::{BuildStreamError, DeviceNameError, PlayStreamError};
use rtp_rs::RtpReaderError;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

#[derive(Error, Debug)]
pub enum SdpPlayerError {
    #[error("invalid bit depth: {0}")]
    InvalidBitDepth(String),
    #[error("malformed sdp file: {0}")]
    MalformedSdpFile(String),
    #[error("invalid sdp version: {0}")]
    InvalidSdpVersion(ParseIntError),
    #[error("invalid packet time: {0}")]
    InvalidPacketTime(ParseFloatError),
    #[error("invalid IP address: {0}")]
    InvalidIP(AddrParseError),
    #[error("invalid payload ID: {0}")]
    InvalidPayloadId(ParseIntError),
    #[error("invalid port: {0}")]
    InvalidPort(ParseIntError),
    #[error("invalid channels: {0}")]
    InvalidChannels(ParseIntError),
    #[error("invalid sample rate: {0}")]
    InvalidSampleRate(ParseIntError),
    #[error("invalid buffer multiplier: {0}")]
    InvalidBufferMultiplier(ParseIntError),
    #[cfg(feature = "net")]
    #[error("reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("io error: {0}")]
    IoError(#[from] io::Error),
    #[error("malformed connection info: {0}")]
    MalformedConnectionInfo(String),
    #[error("malformed ptime attribute: {0}")]
    MalformedPtime(String),
    #[error("malformed rtpmap attribute: {0}")]
    MalformedRtpMap(String),
    #[error("unsupported media type: {0}")]
    UnsupportedMediaType(String),
    #[error("malformed media/transport descriptor: {0}")]
    MalformedMediaTransport(String),
    #[error("no config dir found")]
    NoConfigDir,
    #[error("yaml serde error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("send error: {0}")]
    SendError(#[from] SendError<Vec<u8>>),
    #[error("send error: {0}")]
    StdSendError(#[from] std::sync::mpsc::SendError<Vec<u8>>),
    #[error("rtp reader error")]
    RtpReaderError(RtpReaderError),
    #[error("IPv6 not supported")]
    Ipv6,
    #[error("receiver already started")]
    ReceiverAlreadystarted,
    #[error("device name error: {0}")]
    DeviceNameError(#[from] DeviceNameError),
    #[error("play stream error: {0}")]
    PlayStreamError(#[from] PlayStreamError),
    #[error("build stream error: {0}")]
    BuildStreamError(#[from] BuildStreamError),
    #[error("no default output device found")]
    NoDefaultDevice,
}

impl SdpPlayerError {
    pub fn invalid_sdp_version(e: ParseIntError) -> Self {
        Self::InvalidSdpVersion(e)
    }

    pub fn invalid_packet_time(e: ParseFloatError) -> Self {
        Self::InvalidPacketTime(e)
    }

    pub fn invalid_ip(e: AddrParseError) -> Self {
        Self::InvalidIP(e)
    }

    pub fn invalid_payload_id(e: ParseIntError) -> Self {
        Self::InvalidPayloadId(e)
    }

    pub fn invalid_port(e: ParseIntError) -> Self {
        Self::InvalidPort(e)
    }

    pub fn invalid_channels(e: ParseIntError) -> Self {
        Self::InvalidChannels(e)
    }

    pub fn invalid_sample_rate(e: ParseIntError) -> Self {
        Self::InvalidSampleRate(e)
    }

    pub fn invalid_buffer_multiplier(e: ParseIntError) -> Self {
        Self::InvalidBufferMultiplier(e)
    }
}

pub type SdpPlayerResult<T> = Result<T, SdpPlayerError>;
