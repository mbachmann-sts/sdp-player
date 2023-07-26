use crate::{
    error::{SdpPlayerError, SdpPlayerResult},
    SessionDescriptor,
};
use rtp_rs::RtpReader;
use std::net::Ipv4Addr;
use tokio::{
    net::UdpSocket,
    select, spawn,
    sync::{
        broadcast,
        mpsc::{self},
    },
    time::Instant,
};

pub struct Stream {
    pub descriptor: SessionDescriptor,
    pub socket: Option<UdpSocket>,
}

impl Stream {
    pub async fn new(
        descriptor: SessionDescriptor,
        local_address: Ipv4Addr,
    ) -> SdpPlayerResult<Self> {
        let socket = {
            let socket_addr = format!("{}:{}", local_address, descriptor.multicast_port);
            log::info!("Binding to local address {socket_addr}");
            let socket = UdpSocket::bind(socket_addr).await?;
            log::info!("Joining multicast group {}", descriptor.multicast_address);
            socket.join_multicast_v4(descriptor.multicast_address, local_address)?;
            socket
        };

        Ok(Stream {
            descriptor,
            socket: Some(socket),
        })
    }

    pub async fn play(
        &mut self,
        stop: broadcast::Sender<()>,
    ) -> SdpPlayerResult<mpsc::UnboundedReceiver<Vec<u8>>> {
        let mut buf = [0; 102400];

        let mut start = Instant::now();
        let mut counter = 0;

        let (tx, rx) = mpsc::unbounded_channel();

        let socket = self
            .socket
            .take()
            .ok_or(SdpPlayerError::ReceiverAlreadystarted)?;

        let mut stop = stop.subscribe();

        spawn(async move {
            let mut previous_sequence_number = None;
            loop {
                select! {
                    _ = stop.recv() => { break; },
                    recv = receive_rtp_payload(&socket, &mut buf) => {
                        match recv {
                            Ok(Some((payload,sequence_number))) => {

                                if let Some(previous_sequence_number) = previous_sequence_number {
                                    let diff = sequence_number - previous_sequence_number;
                                    if diff < 1 && !(sequence_number == 0 && previous_sequence_number == 65535) {
                                        log::warn!("Inconsistent RTP sequence number '{sequence_number}', previous was {previous_sequence_number}")
                                    } else if diff > 1 {
                                        log::warn!("Detected packet loss, {} packet(s) were not received", diff-1);
                                    }
                                }
                                previous_sequence_number = Some(sequence_number);

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
                                if let Err(e) = tx.send(payload) {
                                    log::error!("Error forwarding received data: {e}");
                                    log::warn!("Stopping receiver.");
                                    break;
                                }
                            }
                            Ok(None) => (),
                            Err(e) => {
                                log::error!("Error receiving data: {e}");
                                log::warn!("Stopping receiver.");
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }
}

async fn receive_rtp_payload(
    sock: &UdpSocket,
    buf: &mut [u8],
) -> SdpPlayerResult<Option<(Vec<u8>, i32)>> {
    let len = sock.recv(buf).await?;
    if len > 0 {
        let rtp = RtpReader::new(&buf[0..len]).map_err(|e| SdpPlayerError::RtpReaderError(e))?;
        let end = rtp.payload().len() - rtp.padding().unwrap_or(0) as usize;
        let data = (&rtp.payload()[0..end]).to_owned();
        let sequence_number: u16 = rtp.sequence_number().into();
        Ok(Some((data, sequence_number as i32)))
    } else {
        Ok(None)
    }
}
