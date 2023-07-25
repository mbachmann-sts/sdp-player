use anyhow::{anyhow, Ok};
use clap::Parser;
use sdp_player::{
    audio::play,
    preset::{load_presets, save_preset, Preset},
    sdp::{session_descriptor_from_sdp_file, session_descriptor_from_sdp_url},
    stream::Stream,
    BitDepth, SessionDescriptor,
};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
};
use tokio::sync::broadcast;
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// SDP URL
    #[arg(short, long)]
    url: Option<Url>,

    /// SDP file
    #[arg(short, long)]
    file: Option<PathBuf>,

    /// multicast address
    #[arg(short, long)]
    multicast_address: Option<SocketAddrV4>,

    /// bit depth
    #[arg(short, long, default_value_t = BitDepth::L16)]
    bit_depth: BitDepth,

    /// channel count
    #[arg(short, long, default_value_t = 2)]
    channels: u16,

    /// sample rate
    #[arg(short, long, default_value_t = 48000)]
    sample_rate: u32,

    /// packet time
    #[arg(short, long, default_value_t = 1.0)]
    time: f32,

    /// preset
    #[clap(index = 1)]
    preset: Option<String>,

    /// save session as preset with given name
    #[clap(long)]
    save: Option<String>,

    /// list presets and exit
    #[clap(long)]
    ls: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    let args = Args::parse();

    if args.ls {
        let presets = load_presets().await?;
        for preset in presets.keys() {
            println!("{preset}");
        }
        return Ok(());
    }

    let (tx_stop, _rx_stop) = broadcast::channel(1);

    if let Some(preset) = args.preset {
        play_preset(preset, tx_stop).await?;
    } else if let Some(sdp_url) = args.url {
        play_sdp_url(&sdp_url, tx_stop).await?;
    } else if let Some(sdp_file) = args.file {
        let sdp_file = sdp_file.canonicalize()?;
        if let Some(name) = args.save {
            let preset = Preset {
                name,
                local_sdp_file: Some(sdp_file.to_owned()),
                ..Default::default()
            };
            if let Err(e) = save_preset(preset).await {
                log::error!("Could not save preset: {e}");
            }
        }
        play_sdp_file(&sdp_file, tx_stop).await?;
    } else if let Some(multicast_address) = args.multicast_address {
        let channels = args.channels;
        let bit_depth = args.bit_depth;
        let sample_rate = args.sample_rate;
        let packet_time = args.time;
        if let Some(name) = args.save {
            let preset = Preset {
                name,
                custom_stream: Some(SessionDescriptor {
                    bit_depth: bit_depth.clone(),
                    channels,
                    multicast_address,
                    sample_rate,
                    packet_time,
                }),
                ..Default::default()
            };
            if let Err(e) = save_preset(preset).await {
                log::error!("Could not save preset: {e}");
            }
        }
        play_descriptor(
            SessionDescriptor {
                multicast_address,
                bit_depth,
                channels,
                sample_rate,
                packet_time,
            },
            tx_stop,
        )
        .await?;
    }

    Ok(())
}

async fn play_preset(preset: String, stop: broadcast::Sender<()>) -> anyhow::Result<()> {
    log::info!("Playing stream from preset '{preset}'");
    let presets = load_presets().await?;
    if let Some(preset) = presets.get(&preset) {
        if let Some(sdp_url) = &preset.sdp_url {
            play_sdp_url(sdp_url, stop).await?;
        } else if let Some(sdp_file) = &preset.local_sdp_file {
            play_sdp_file(&sdp_file, stop).await?;
        } else if let Some(sd) = preset.custom_stream.clone() {
            play_descriptor(sd, stop).await?;
        }
        Ok(())
    } else {
        Err(anyhow!("No preset with name '{preset}' found."))
    }
}

async fn play_sdp_url(url: &Url, stop: broadcast::Sender<()>) -> anyhow::Result<()> {
    log::info!("Playing stream from SDP url '{url}'");

    let sd = session_descriptor_from_sdp_url(url).await?;
    do_play_descriptor(sd, stop).await
}

async fn play_sdp_file(sdp_file: &Path, stop: broadcast::Sender<()>) -> anyhow::Result<()> {
    log::info!(
        "Playing stream from SDP file '{}'",
        sdp_file.as_os_str().to_string_lossy()
    );

    let sd = session_descriptor_from_sdp_file(sdp_file).await?;
    do_play_descriptor(sd, stop).await
}

async fn play_descriptor(sd: SessionDescriptor, stop: broadcast::Sender<()>) -> anyhow::Result<()> {
    log::info!(
        "Playing custom stream '{} {}/{}/{}'",
        sd.multicast_address,
        sd.bit_depth,
        sd.sample_rate,
        sd.channels
    );

    do_play_descriptor(sd, stop).await
}

async fn do_play_descriptor(
    sd: SessionDescriptor,
    stop: broadcast::Sender<()>,
) -> anyhow::Result<()> {
    let local_address = Ipv4Addr::UNSPECIFIED;
    let stream = Stream::new(sd, local_address).await?;
    play(stream, stop).await?;

    Ok(())
}
