use anyhow::{anyhow, Ok};
use clap::Parser;
use sdp_player::{
    audio::{play, Stream},
    preset::{load_presets, save_preset, CustomStreamSettings, Preset},
    sdp::{sdp_from_file, sdp_from_url, BitDepth},
    stream::{subscribe, subscribe_sdp},
};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
};
use tokio::{spawn, sync::mpsc};
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

    if let Some(preset) = args.preset {
        play_preset(preset).await?;
    } else if let Some(sdp_url) = args.url {
        play_sdp_url(&sdp_url).await?;
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
        play_sdp_file(&sdp_file).await?;
    } else if let Some(multicast_address) = args.multicast_address {
        let channels = args.channels;
        let bit_depth = args.bit_depth;
        let sample_rate = args.sample_rate;
        let packet_time = args.time;
        if let Some(name) = args.save {
            let preset = Preset {
                name,
                custom_stream: Some(CustomStreamSettings {
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
        play_stream(
            multicast_address,
            channels,
            bit_depth,
            sample_rate,
            packet_time,
        )
        .await?;
    }

    Ok(())
}

async fn play_preset(preset: String) -> anyhow::Result<()> {
    log::info!("Playing stream from preset '{preset}'");
    let presets = load_presets().await?;
    if let Some(preset) = presets.get(&preset) {
        if let Some(sdp_url) = &preset.sdp_url {
            play_sdp_url(sdp_url).await?;
        } else if let Some(sdp_file) = &preset.local_sdp_file {
            play_sdp_file(&sdp_file).await?;
        } else if let Some(CustomStreamSettings {
            multicast_address,
            bit_depth,
            channels,
            sample_rate,
            packet_time,
        }) = &preset.custom_stream
        {
            play_stream(
                *multicast_address,
                *channels,
                bit_depth.clone(),
                *sample_rate,
                *packet_time,
            )
            .await?;
        }
        Ok(())
    } else {
        Err(anyhow!("No preset with name '{preset}' found."))
    }
}

async fn play_sdp_url(url: &Url) -> anyhow::Result<()> {
    log::info!("Playing stream from SDP url '{url}'");

    let local_ip = Ipv4Addr::UNSPECIFIED;
    let sdp = sdp_from_url(url).await?;
    let (tx, rx) = mpsc::unbounded_channel();
    spawn(subscribe_sdp(sdp.clone(), tx, local_ip));
    play(Stream::from_sdp(rx, sdp)).await?;

    Ok(())
}

async fn play_sdp_file(sdp_file: &Path) -> anyhow::Result<()> {
    log::info!(
        "Playing stream from SDP file '{}'",
        sdp_file.as_os_str().to_string_lossy()
    );

    let local_ip = Ipv4Addr::UNSPECIFIED;
    let sdp = sdp_from_file(sdp_file).await?;
    let (tx, rx) = mpsc::unbounded_channel();
    spawn(subscribe_sdp(sdp.clone(), tx, local_ip));
    play(Stream::from_sdp(rx, sdp)).await?;

    Ok(())
}

async fn play_stream(
    multicast_address: SocketAddrV4,
    channels: u16,
    bit_depth: BitDepth,
    sample_rate: u32,
    packet_time: f32,
) -> anyhow::Result<()> {
    log::info!("Playing custom stream '{multicast_address} {bit_depth}/{sample_rate}/{channels}'");

    let local_ip = Ipv4Addr::UNSPECIFIED;
    let (tx, rx) = mpsc::unbounded_channel();
    spawn(subscribe(
        *multicast_address.ip(),
        multicast_address.port(),
        tx,
        local_ip,
    ));
    play(Stream::new(
        rx,
        channels,
        sample_rate,
        bit_depth,
        packet_time,
    ))
    .await?;

    Ok(())
}
