use sdp_player::{
    audio::{play, Stream},
    sdp::{sdp_from_url, BitDepth},
    stream::{subscribe, subscribe_sdp},
};
use std::{env, net::Ipv4Addr};
use tokio::{spawn, sync::mpsc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();
    let mut args = env::args();
    let _ = args.next().unwrap();

    _sdp().await?;
    // _audinate_aes67().await?;

    Ok(())
}

async fn _sdp() -> anyhow::Result<()> {
    let local_ip = Ipv4Addr::UNSPECIFIED;
    let sdp_url = env::var("SDP_URL").expect("SDP_URL not set");
    let sdp = sdp_from_url(&sdp_url).await?;
    let (tx, rx) = mpsc::unbounded_channel();
    spawn(subscribe_sdp(sdp.clone(), tx, local_ip));
    play(Stream::from_sdp(rx, sdp)).await?;
    Ok(())
}

async fn _audinate_aes67() -> anyhow::Result<()> {
    let local_ip = Ipv4Addr::UNSPECIFIED;
    let (tx, rx) = mpsc::unbounded_channel();
    let multicast_address = env::var("MULTICAST_ADDRESS")
        .expect("MULTICAST_ADDRESS not set")
        .parse()?;
    let multicast_port = env::var("MULTICAST_PORT")
        .expect("MULTICAST_PORT not set")
        .parse()?;
    spawn(subscribe(multicast_address, multicast_port, tx, local_ip));
    play(Stream::new(rx, 2, 48000, BitDepth::L24, 1.0)).await?;
    Ok(())
}
