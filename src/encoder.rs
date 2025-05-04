use std::iter::repeat;

use crate::{
    gc_dspadpcm::{dsp_correlate_coefs, dsp_encode_frame, PACKET_BYTES, PACKET_SAMPLES},
    structs::{AdpcmChannelInformation, Channels, Head1, TrackDescription},
    BrstmInfoWithData, BrstmInformation,
};

const BLOCK_SIZE: usize = 8192;

// TODO: use from std when it's stable
pub const fn div_ceil(lhs: usize, rhs: usize) -> usize {
    let d = lhs / rhs;
    let r = lhs % rhs;
    if r > 0 && rhs > 0 {
        d + 1
    } else {
        d
    }
}

struct BrstmStreamEncoder<'a> {
    // input
    samples: &'a [i16],
    samples_until_loop_point: Option<usize>,
    // work
    prev_samples: [i16; 2],
    is_first: bool,

    // output
    coefs: [[i16; 2]; 8],
    initial_predictor: u8,
    loop_predictor: u8,
    loop_history_samples: [i16; 2],
}

impl<'a> BrstmStreamEncoder<'a> {
    pub fn init(samples: &'a [i16], loop_point: u32) -> Self {
        let loop_point: usize = loop_point.try_into().unwrap();

        let coefs = dsp_correlate_coefs(samples);

        // gracefully handle the case when the loop point is 0
        let loop_history_samples = [
            samples
                .get(loop_point.wrapping_sub(2))
                .copied()
                .unwrap_or(0),
            samples
                .get(loop_point.wrapping_sub(1))
                .copied()
                .unwrap_or(0),
        ];

        Self {
            samples,
            coefs,
            prev_samples: [0; 2],
            loop_history_samples,
            samples_until_loop_point: Some(loop_point),
            loop_predictor: 0,
            is_first: false,
            initial_predictor: 0,
        }
    }

    /// returns number of writes without padding when done (it writes padding)
    pub fn pull_chunk(
        &mut self,
        adpcm_bytes: &mut Vec<u8>,
        data_bytes: &mut Vec<u8>,
    ) -> Option<(usize, usize)> {
        // we have 8192 bytes to fill

        let mut conv_samps = [0i16; 16];
        conv_samps[0] = self.prev_samples[0];
        conv_samps[1] = self.prev_samples[1];
        
        adpcm_bytes.extend_from_slice(&self.prev_samples[0].to_be_bytes());
        adpcm_bytes.extend_from_slice(&self.prev_samples[1].to_be_bytes());
        self.prev_samples[0] = self.samples.get(BLOCK_SIZE / PACKET_BYTES * PACKET_SAMPLES - 2).copied().unwrap_or(0);
        self.prev_samples[1] = self.samples.get(BLOCK_SIZE / PACKET_BYTES * PACKET_SAMPLES - 1).copied().unwrap_or(0);
        // each packet is 8 bytes
        for p in 0..BLOCK_SIZE / PACKET_BYTES {
            // let num_samples = self.samples.len().min(PACKET_SAMPLES);
            // the first 2 samples are from the previous packet, if the stream ends first the rest is zero filled
            for (conv_samp, samp) in conv_samps
                .iter_mut()
                .skip(2)
                .zip(self.samples.iter().chain(repeat(&0)))
            {
                *conv_samp = *samp;
            }

            let block = dsp_encode_frame(&mut conv_samps, PACKET_SAMPLES, &self.coefs);
            data_bytes.extend_from_slice(&block);

            if self.is_first {
                self.is_first = false;
                self.initial_predictor = block[0];
            }

            if let Some(loop_point) = self.samples_until_loop_point {
                if loop_point < PACKET_SAMPLES {
                    self.samples_until_loop_point = None;
                    self.loop_predictor = block[0]
                } else {
                    self.samples_until_loop_point = Some(loop_point - PACKET_SAMPLES);
                }
            }

            conv_samps[0] = conv_samps[14];
            conv_samps[1] = conv_samps[15];

            if self.samples.len() > PACKET_SAMPLES {
                self.samples = &self.samples[PACKET_SAMPLES..];
            } else {
                // there aren't enough samples to fill this block, so encoding this stream is finished
                let bytes_written = (p + 1) * PACKET_BYTES;
                let samples = p * PACKET_SAMPLES + self.samples.len();
                // pad to 32 bytes
                let rem = bytes_written % 32;
                if rem != 0 {
                    for _ in 0..(32 - rem) {
                        data_bytes.push(0);
                    }
                }
                return Some((bytes_written, samples));
            }
        }
        None
    }

    pub fn get_adpcm_channel_info(&self) -> AdpcmChannelInformation {
        let mut adpcm_coefficients = [0; 16];
        for (dst, src) in adpcm_coefficients
            .iter_mut()
            .zip(self.coefs.iter().flatten())
        {
            *dst = *src;
        }
        AdpcmChannelInformation {
            // TODO: remove unsafe here
            adpcm_coefficients,
            gain: 0,
            history_sample1: 0,
            history_sample2: 0,
            initial_predictor: self.initial_predictor.into(), // TODO
            loop_history_sample1: self.loop_history_samples[0],
            loop_history_sample2: self.loop_history_samples[1],
            loop_predictor: self.loop_predictor.into(),
            ..Default::default()
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum EncodingError {
    #[error("No channels, at least 2 are needed")]
    EmptyChannels,
    #[error("Only stereo is supported, but channel count {0} isn't divisible by 2")]
    UnevenChannelCount(usize),
    #[error("Too many channels: {0}, only 16 are supported")]
    TooHighChannelCount(usize),
    #[error("Loop point {loop_point} is greater than the amount of samples {size}")]
    LoopOutOfBounds { loop_point: usize, size: usize },
    #[error("All Channels must have the same length, got {0:?}")]
    MissmatchedLengths(Vec<usize>),
}

pub fn encode_brstm(
    channels: &[Vec<i16>],
    sampling_rate: u16,
    loop_point: Option<u32>,
) -> Result<BrstmInfoWithData, EncodingError> {
    // make sure all channels have the same length
    let mut lengths_iter = channels.iter().map(|c| c.len());
    let sample_count = lengths_iter.next().ok_or(EncodingError::EmptyChannels)?;
    if !lengths_iter.all(|len| len == sample_count) {
        return Err(EncodingError::MissmatchedLengths(
            channels.iter().map(|c| c.len()).collect(),
        ));
    }
    if channels.len() % 2 != 0 && channels.len() != 1 {
        return Err(EncodingError::UnevenChannelCount(channels.len()));
    }
    if channels.len() > 16 {
        return Err(EncodingError::TooHighChannelCount(channels.len()));
    }
    if let Some(loop_point) = loop_point {
        if loop_point as usize > sample_count {
            return Err(EncodingError::LoopOutOfBounds {
                loop_point: loop_point as _,
                size: sample_count,
            });
        }
    }

    let mut data_bytes = Vec::new();
    let mut adpcm_bytes = Vec::new();
    let mut channel_encoders: Vec<_> = channels
        .iter()
        .map(|channel| BrstmStreamEncoder::init(channel, loop_point.unwrap_or(0)))
        .collect();

    let mut block_count = 0;
    let (final_block_size, final_block_samples) = loop {
        let mut final_chunk_unpadded = None;
        for encoder in &mut channel_encoders {
            // not sure if there is a better way to write this, but we have to stop once all
            // streams reach the final block, which
            let ret = encoder.pull_chunk(&mut adpcm_bytes, &mut data_bytes);
            // just debug to make sure all have the same length
            // if let (Some(a), Some(b)) = (&final_chunk_unpadded, &ret) {
            //     assert_eq!(a, b);
            // }
            final_chunk_unpadded = ret;
        }
        block_count += 1;
        if let Some(size) = final_chunk_unpadded {
            break size;
        }
    };

    // after the encoding is done, grab the channel info, because encoding fills in the predictors
    let channel_infos: Vec<_> = channel_encoders
        .iter()
        .map(BrstmStreamEncoder::get_adpcm_channel_info)
        .collect();

    let tracks = if channels.len() == 1 {
        vec![TrackDescription{
            channels: Channels::Mono(0),
            ..Default::default()
        }]
    } else { (0..(channels.len() as u8 / 2))
        .map(|t| TrackDescription {
            channels: Channels::Stereo(t * 2, t * 2 + 1),
            ..Default::default()
        })
        .collect()};

    let out_brstm = BrstmInformation {
        channels: channel_infos,
        tracks,
        info: Head1 {
            codec: 2, // ADPCM
            sample_rate: sampling_rate,
            loop_flag: loop_point.is_some().into(),
            num_channels: channels.len() as u8,
            loop_start: loop_point.unwrap_or(0),
            total_samples: sample_count as u32,
            adpc_bytes_per_entry: 4,
            adpc_samples_per_entry: 14336, // 8192 / 8 * 14
            blocks_samples: 14336,
            blocks_size: 8192,
            final_block_samples: final_block_samples.try_into().unwrap(),
            final_block_size: final_block_size.try_into().unwrap(),
            final_block_size_padded: ((final_block_size + 31) & !31).try_into().unwrap(),
            total_blocks: block_count as u32,
            // filled in later
            audio_offset: 0,
        },
        // filled in later
        adpcm_offset: 0,
        adpcm_size: 0,
        data_offset: 0,
        data_size: 0,
    };
    Ok(BrstmInfoWithData {
        adpcm_bytes,
        data_bytes,
        info: out_brstm,
    })
}
