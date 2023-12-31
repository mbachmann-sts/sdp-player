use crate::error::{SdpPlayerError, SdpPlayerResult};
use crate::stream::Stream;
use crate::BitDepth;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{traits::HostTrait, FromSample, SizedSample};
use cpal::{SampleRate, StreamConfig};
use std::fmt::Debug;
use std::{env, thread};
use tokio::sync::broadcast;
use tokio::time::Instant;
use tokio::{select, spawn};

pub async fn play(mut stream: Stream, stop: broadcast::Sender<()>) -> SdpPlayerResult<()> {
    let host = cpal::default_host();
    let descriptor = stream.descriptor.clone();

    let mut stream_rx = stream.play(stop.clone()).await?;

    if let Some(device) = host.default_output_device() {
        log::info!("Output device: {}", device.name()?);

        let default_config = device.default_output_config().unwrap();
        log::info!("Default output config: {:?}", default_config);

        let buffer_multiplier: u32 = env::var("BUFFER_MULTIPLIER")
            .ok()
            .and_then(|m| m.parse::<u32>().ok())
            .unwrap_or(45 * descriptor.channels as u32);

        let packet_size = descriptor.buffer_size();
        let buffer_frames = (packet_size as f32
            / descriptor.channels as f32
            / descriptor.bit_depth.bits() as f32) as u32;
        let receiver_buffer_frames = buffer_frames * buffer_multiplier;

        log::debug!(
            "Buffer size: {} frames / {} ms; requested receiver buffer size: {} frames / {} ms)",
            buffer_frames,
            descriptor.packet_time,
            receiver_buffer_frames,
            descriptor.packet_time * buffer_multiplier as f32
        );

        let config = StreamConfig {
            buffer_size: cpal::BufferSize::Fixed(receiver_buffer_frames),
            channels: descriptor.channels,
            sample_rate: SampleRate(descriptor.sample_rate),
        };

        log::info!("Output config: {:?}", config);

        let (tx, rx) = std::sync::mpsc::channel();
        let (meter_tx, meter_rx) = std::sync::mpsc::channel();

        let converter = match descriptor.bit_depth {
            BitDepth::L16 => l16_samples,
            BitDepth::L24 => l24_samples,
            BitDepth::L32 => l32_samples,
            BitDepth::FloatingPoint => f32_samples,
        };

        let (tx_stop, rx_stop) = std::sync::mpsc::channel();
        let mut stop_run = stop.subscribe();
        spawn(async move {
            stop_run.recv().await.ok();
            tx_stop.send(()).ok();
        });
        thread::spawn(move || {
            match default_config.sample_format() {
                cpal::SampleFormat::I8 => {
                    run::<i8>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                cpal::SampleFormat::I16 => {
                    run::<i16>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                // cpal::SampleFormat::I24 => run::<I24>(&device, &config),
                cpal::SampleFormat::I32 => {
                    run::<i32>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                // cpal::SampleFormat::I48 => run::<I48>(&device, &config),
                cpal::SampleFormat::I64 => {
                    run::<i64>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                cpal::SampleFormat::U8 => {
                    run::<u8>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                cpal::SampleFormat::U16 => {
                    run::<u16>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                // cpal::SampleFormat::U24 => run::<U24>(&device, &config),
                cpal::SampleFormat::U32 => {
                    run::<u32>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                // cpal::SampleFormat::U48 => run::<U48>(&device, &config),
                cpal::SampleFormat::U64 => {
                    run::<u64>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                cpal::SampleFormat::F32 => {
                    run::<f32>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                cpal::SampleFormat::F64 => {
                    run::<f64>(&device, &config, rx, converter, meter_tx, rx_stop)
                }
                sample_format => panic!("Unsupported sample format '{sample_format}'"),
            }
        });

        let sample_rate = descriptor.sample_rate;
        let channels = descriptor.channels as usize;
        thread::spawn(move || {
            let mut start = Instant::now();
            let mut level = 0.0;

            while let Ok(samples) = meter_rx.recv() {
                let buffer_size = samples.len();
                for s in samples {
                    let l = s.abs();
                    if l > level {
                        level = l;
                    }
                }
                if start.elapsed().as_secs_f32() >= 1.0 {
                    let db = 20.0 * level.log10();
                    let actual_buffer_frames = buffer_size / channels;
                    log::debug!("Audio level: {db:.2} dB");
                    log::debug!(
                        "Actual receiver buffer size: {} frames / {} ms",
                        actual_buffer_frames,
                        (actual_buffer_frames * 1000) / sample_rate as usize
                    );
                    start = Instant::now();
                    level = 0.0;
                }
            }
        });

        let mut stop = stop.subscribe();

        loop {
            select! {
                recv = stream_rx.recv() => {
                    if let Some(packet) = recv {
                        tx.send(packet)?;
                    } else {
                        break;
                    }
                }
                _ = stop.recv() => { break; }
            }
        }

        log::info!("Playback stopped.");

        Ok(())
    } else {
        Err(SdpPlayerError::NoDefaultDevice)
    }
}

pub fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    converter: fn(&[u8]) -> Vec<f32>,
    meter_tx: std::sync::mpsc::Sender<Vec<f32>>,
    stop: std::sync::mpsc::Receiver<()>,
) -> SdpPlayerResult<()>
where
    T: SizedSample + FromSample<f32> + Send + Debug + 'static,
{
    let err_fn = |err| log::error!("an error occurred on stream: {}", err);

    let mut ready_samples = Vec::new();

    let data_callback = move |buf: &mut [T], _: &cpal::OutputCallbackInfo| {
        let buffer_size = buf.len();

        while ready_samples.len() < buffer_size {
            if let Ok(new_data) = rx.recv() {
                let new_samples = converter(&new_data);
                ready_samples.extend(new_samples);
            } else {
                break;
            }
        }

        if let Err(e) = meter_tx.send((&ready_samples[0..buffer_size]).to_owned()) {
            log::error!("Error forwarding meter values: {e}");
        }

        let mut output = buf.iter_mut();

        for s in ready_samples.drain(0..buffer_size.min(ready_samples.len())) {
            let sample = output.next().expect("buffer overflow");
            *sample = T::from_sample::<f32>(s);
        }
    };

    let stream = device.build_output_stream(config, data_callback, err_fn, None)?;
    stream.play()?;

    stop.recv().ok();

    Ok(())
}

fn l16_samples(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();

    for sample_bytes in bytes.chunks(2) {
        let mut sample = [0; 2];
        for (i, b) in sample_bytes.iter().enumerate() {
            sample[i] = *b;
        }
        let val = i16::from_be_bytes(sample);
        let float = (val as f64) / (i16::MAX as f64);
        out.push(float as f32);
    }

    out
}

fn l24_samples(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();

    for sample_bytes in bytes.chunks(3) {
        let mut sample = [0; 4];
        for (i, b) in sample_bytes.iter().enumerate() {
            sample[i] = *b;
        }
        let val = i32::from_be_bytes(sample);
        let float = val as f64 / i32::MAX as f64;
        out.push(float as f32);
    }

    out
}

fn l32_samples(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();

    for sample_bytes in bytes.chunks(4) {
        let mut sample = [0; 4];
        for (i, b) in sample_bytes.iter().enumerate() {
            sample[i] = *b;
        }
        let val = i32::from_be_bytes(sample);
        let float = val as f64 / i32::MAX as f64;
        out.push(float as f32);
    }

    out
}

fn f32_samples(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();

    for sample_bytes in bytes.chunks(3) {
        let mut sample = [0; 4];
        for (i, b) in sample_bytes.iter().enumerate() {
            sample[i] = *b;
        }
        let val = f32::from_be_bytes(sample);
        out.push(val);
    }

    out
}
