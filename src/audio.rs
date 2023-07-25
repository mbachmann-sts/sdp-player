use std::fmt::Debug;
use std::{env, thread};

use crate::sdp::{BitDepth, Sdp};
use anyhow::anyhow;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{traits::HostTrait, FromSample, SizedSample};
use cpal::{SampleRate, StreamConfig};
use tokio::sync::mpsc;

pub struct Stream {
    rx: mpsc::UnboundedReceiver<Vec<u8>>,
    channels: u16,
    sample_rate: u32,
    bit_depth: BitDepth,
    packet_time: f32,
}

impl Stream {
    pub fn new(
        rx: mpsc::UnboundedReceiver<Vec<u8>>,
        channels: u16,
        sample_rate: u32,
        bit_depth: BitDepth,
        packet_time: f32,
    ) -> Self {
        Self {
            bit_depth,
            channels,
            rx,
            sample_rate,
            packet_time,
        }
    }

    pub fn from_sdp(
        rx: mpsc::UnboundedReceiver<Vec<u8>>,
        Sdp {
            version: _,
            multicast_port: _,
            multicast_address: _,
            payload_id: _,
            packet_time,
            bit_depth,
            sample_rate,
            channels,
        }: Sdp,
    ) -> Self {
        Self {
            bit_depth,
            channels,
            rx,
            sample_rate,
            packet_time,
        }
    }

    fn buffer_size(&self) -> u32 {
        let packet_time = self.packet_time;
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        ((channels as f64 * packet_time as f64 * sample_rate as f64) / 1_000.0) as u32
    }
}

pub async fn play(mut stream: Stream) -> anyhow::Result<()> {
    let host = cpal::default_host();

    if let Some(device) = host.default_output_device() {
        log::info!("Output device: {}", device.name()?);

        let default_config = device.default_output_config().unwrap();
        log::info!("Default output config: {:?}", default_config);

        let buffer_multiplier: u32 = env::var("BUFFER_MULTIPLIER")
            .unwrap_or("100".to_owned())
            .parse()?;

        let config = StreamConfig {
            buffer_size: cpal::BufferSize::Fixed(stream.buffer_size() * buffer_multiplier),
            channels: stream.channels,
            sample_rate: SampleRate(stream.sample_rate),
        };

        log::info!("Output config: {:?}", config);

        let (tx, rx) = std::sync::mpsc::channel();

        let converter = match stream.bit_depth {
            BitDepth::L16 => l16_samples,
            BitDepth::L24 => l24_samples,
            BitDepth::L32 => l32_samples,
            BitDepth::FloatingPoint => f32_samples,
        };

        thread::spawn(move || {
            match default_config.sample_format() {
                cpal::SampleFormat::I8 => run::<i8>(&device, &config, rx, converter),
                cpal::SampleFormat::I16 => run::<i16>(&device, &config, rx, converter),
                // cpal::SampleFormat::I24 => run::<I24>(&device, &config),
                cpal::SampleFormat::I32 => run::<i32>(&device, &config, rx, converter),
                // cpal::SampleFormat::I48 => run::<I48>(&device, &config),
                cpal::SampleFormat::I64 => run::<i64>(&device, &config, rx, converter),
                cpal::SampleFormat::U8 => run::<u8>(&device, &config, rx, converter),
                cpal::SampleFormat::U16 => run::<u16>(&device, &config, rx, converter),
                // cpal::SampleFormat::U24 => run::<U24>(&device, &config),
                cpal::SampleFormat::U32 => run::<u32>(&device, &config, rx, converter),
                // cpal::SampleFormat::U48 => run::<U48>(&device, &config),
                cpal::SampleFormat::U64 => run::<u64>(&device, &config, rx, converter),
                cpal::SampleFormat::F32 => run::<f32>(&device, &config, rx, converter),
                cpal::SampleFormat::F64 => run::<f64>(&device, &config, rx, converter),
                sample_format => panic!("Unsupported sample format '{sample_format}'"),
            }
        });

        while let Some(packet) = stream.rx.recv().await {
            tx.send(packet)?;
        }

        Ok(())
    } else {
        Err(anyhow!("No default output device."))
    }
}

pub fn run<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
    converter: fn(&[u8]) -> Vec<f32>,
) -> Result<(), anyhow::Error>
where
    T: SizedSample + FromSample<f32> + Send + Debug + 'static,
{
    let err_fn = |err| log::error!("an error occurred on stream: {}", err);

    let mut ready_samples = Vec::new();

    let data_callback = move |buf: &mut [T], _: &cpal::OutputCallbackInfo| {
        let buffer_size = buf.len();

        while ready_samples.len() < buffer_size {
            let new_data = rx.recv().expect("no more audio data");
            let new_samples = converter(&new_data);
            ready_samples.extend(new_samples);
        }

        let mut output = buf.iter_mut();

        for s in ready_samples.drain(0..buffer_size) {
            let sample = output.next().expect("buffer overflow");
            *sample = T::from_sample::<f32>(s);
        }
    };

    let stream = device.build_output_stream(config, data_callback, err_fn, None)?;
    stream.play()?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn l16_samples(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();

    for sample_bytes in bytes.chunks(2) {
        let mut sample = [0; 2];
        for (i, b) in sample_bytes.iter().enumerate() {
            sample[i] = *b;
        }
        let val = i16::from_be_bytes(sample);
        let float = val as f64 / i16::MAX as f64;
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