use crate::sdp::Sdp;
use rtp_rs::{RtpReader, RtpReaderError};
use std::{
    io,
    net::{AddrParseError, Ipv4Addr},
    num::ParseIntError,
};
use thiserror::Error;
use tokio::{
    net::UdpSocket,
    sync::mpsc::{self, error::SendError},
    time::Instant,
};

pub async fn subscribe_sdp(
    sdp: Sdp,
    bytes_received: mpsc::UnboundedSender<Vec<u8>>,
    local_ip: Ipv4Addr,
) -> StreamResult<()> {
    let port = sdp.multicast_port;
    match sdp.multicast_address {
        std::net::IpAddr::V4(addr) => subscribe(addr, port, bytes_received, local_ip).await,
        // IPv6 not yet supported
        std::net::IpAddr::V6(_) => return Err(StreamError::StreamError),
    }
}

pub async fn subscribe(
    multicast_addr: Ipv4Addr,
    multicast_port: u16,
    bytes_received: mpsc::UnboundedSender<Vec<u8>>,
    local_ip: Ipv4Addr,
) -> StreamResult<()> {
    let sock = {
        let socket_addr = format!("{}:{}", local_ip, multicast_port);
        log::info!("Binding to local address {socket_addr}");
        let socket = UdpSocket::bind(socket_addr).await?;
        log::info!("Joining multicast group {multicast_addr}");
        socket.join_multicast_v4(multicast_addr, local_ip)?;
        socket
    };

    let mut buf = [0; 102400];

    let mut start = Instant::now();
    let mut counter = 0;

    loop {
        if let Some(payload) = receive_rtp_payload(&sock, &mut buf).await? {
            if start.elapsed().as_secs_f32() >= 1.0 {
                log::debug!(
                    "Receiving {} packets/s; payload size: {}",
                    counter,
                    payload.len()
                );
                counter = 0;
                start = Instant::now();
            } else {
                counter += 1;
            }
            bytes_received.send(payload)?;
        }
    }
}

async fn receive_rtp_payload(sock: &UdpSocket, buf: &mut [u8]) -> StreamResult<Option<Vec<u8>>> {
    let len = sock.recv(buf).await?;
    if len > 0 {
        let rtp = RtpReader::new(&buf[0..len]).map_err(|e| StreamError::RtpReaderError(e))?;
        let end = rtp.payload().len() - rtp.padding().unwrap_or(0) as usize;
        let data = (&rtp.payload()[0..end]).to_owned();
        Ok(Some(data))
    } else {
        Ok(None)
    }
}

#[derive(Error, Debug)]
pub enum StreamError {
    #[error("stream error")]
    StreamError,
    #[error("io error")]
    IoError(#[from] io::Error),
    #[error("send error")]
    SendError(#[from] SendError<Vec<u8>>),
    #[error("addr parse error")]
    AddrParseError(#[from] AddrParseError),
    #[error("parse int error")]
    ParseIntError(#[from] ParseIntError),
    #[error("rtp reader error")]
    RtpReaderError(RtpReaderError),
}

pub type StreamResult<T> = Result<T, StreamError>;
