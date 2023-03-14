use std::{
    fs::File,
    io::{Seek, SeekFrom},
    path::Path,
};

use anyhow::bail;
use binrw::{io::BufReader, BinReaderExt};

#[binrw::binread]
#[derive(Debug, Clone, Copy)]
pub struct WavHeader {
    pub audio_format: u16,
    pub channel_count: u16,
    pub sampling_rate: u32,
    pub bytes_per_second: u32,
    pub bytes_per_sample: u16,
    pub bits_per_sample: u16,
}

#[binrw::binread]
#[derive(Debug, Clone, Copy)]
pub struct RiffWavHeader {
    pub magic: [u8; 4],
    pub filesize: u32,
    pub wav: [u8; 4],
    pub fmt: [u8; 4],
}

impl RiffWavHeader {
    pub fn check(&self) -> anyhow::Result<()> {
        if &self.magic != b"RIFF" {
            bail!("Not riff");
        }
        if &self.wav != b"WAVE" {
            bail!("Not wave");
        }
        if &self.fmt != b"fmt " {
            bail!("Not fmt");
        }
        Ok(())
    }
}

pub struct WavData {
    pub channels: Vec<Vec<i16>>,
    pub sample_rate: u16,
}

pub fn read_channels_from_wav<P: AsRef<Path>>(p: P) -> binrw::BinResult<WavData> {
    let mut read = BufReader::new(File::open(p)?);
    let riff_header: RiffWavHeader = read.read_le()?;
    riff_header.check().unwrap();
    read.seek(SeekFrom::Current(4))?;
    let wav_header: WavHeader = read.read_le()?;
    // println!("{wav_header:?}");
    assert!(wav_header.audio_format == 1); // PCM
    read.seek(SeekFrom::Current(4))?;
    let chunksize: u32 = read.read_le()?;
    // dbg!(chunksize);
    let mut channels = vec![Vec::new(); wav_header.channel_count.into()];
    // let mut out = BufWriter::new(File::create("out.bin").unwrap());
    for _ in 0..(chunksize / wav_header.channel_count as u32 / 2) {
        for channel in channels.iter_mut() {
            let val: i16 = read.read_le()?;
            channel.push(val);
        }
    }
    Ok(WavData {
        channels,
        sample_rate: wav_header.sampling_rate.try_into().unwrap(),
    })
}
