pub mod audio;
pub mod error;
pub mod preset;
pub mod sdp;
pub mod stream;

use error::SdpPlayerError;
use serde::{Deserialize, Serialize};
use std::{fmt, net::SocketAddrV4, str::FromStr};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionDescriptor {
    pub multicast_address: SocketAddrV4,
    pub bit_depth: BitDepth,
    pub channels: u16,
    pub sample_rate: u32,
    pub packet_time: f32,
}

impl SessionDescriptor {
    pub fn buffer_size(&self) -> u32 {
        let packet_time = self.packet_time;
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        ((channels as f64 * packet_time as f64 * sample_rate as f64) / 1_000.0) as u32
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
    type Err = SdpPlayerError;

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
            return Err(SdpPlayerError::InvalidBitDepth(s.to_owned()));
        }
    }
}
