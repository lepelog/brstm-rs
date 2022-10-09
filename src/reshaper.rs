use crate::{
    brstm::BrstmInfoWithData,
    structs::{AdpcmChannelInformation, Channels, TrackDescription},
    ReshapeSrc,
};
use thiserror::Error;

#[derive(PartialEq, Eq)]
pub enum AdditionalTrackKind {
    Normal,
    Additive,
}

#[derive(Error, Debug)]
pub enum ReshapeError {
    #[error("Song is not stereo")]
    NotStereo,
    #[error("Referenced Track doesn't exist")]
    TrackNotExistent,
    #[error("Referenced Channel doesn't exist")]
    ChannelNotExistent,
}

pub type AdditionalTracks = [AdditionalTrackKind];

pub fn calc_reshape(original: &AdditionalTracks, new: &AdditionalTracks) -> Vec<ReshapeSrc> {
    // main track always stays
    let mut result = Vec::with_capacity(new.len() + 1);
    result.push(ReshapeSrc::Track(0));
    let mut orig_normal_tracks = original.iter().enumerate().filter_map(|(i, typ)| {
        if *typ == AdditionalTrackKind::Normal {
            // need to add one since the additional tracks start at 1
            Some(i as u8 + 1)
        } else {
            None
        }
    });
    let mut orig_additive_tracks = original.iter().enumerate().filter_map(|(i, typ)| {
        if *typ == AdditionalTrackKind::Additive {
            Some(i as u8 + 1)
        } else {
            None
        }
    });
    // try to find a matching track in the original, otherwise use base track for normal and empty for additive
    for track in new.iter() {
        let reshape_entry = match track {
            AdditionalTrackKind::Normal => {
                ReshapeSrc::Track(orig_normal_tracks.next().unwrap_or(0))
            }
            AdditionalTrackKind::Additive => orig_additive_tracks
                .next()
                .map(ReshapeSrc::Track)
                .unwrap_or(ReshapeSrc::Empty),
        };
        result.push(reshape_entry);
    }
    result
}

pub fn reshape(
    brstm: &mut BrstmInfoWithData,
    track_reshape: &[ReshapeSrc],
) -> Result<(), ReshapeError> {
    if !brstm
        .info
        .tracks
        .iter()
        .all(|track| track.channels.channels() == 2)
    {
        return Err(ReshapeError::NotStereo);
    }
    // first, figure out how channels need to be reshaped
    let mut channel_reshape = Vec::new();
    let mut cur_channel_idx = 0;
    let mut new_tracks = Vec::new();
    let mut new_channels = Vec::new();
    for track in track_reshape.iter() {
        match &track {
            ReshapeSrc::Track(track_ref) => {
                let src_track = brstm
                    .info
                    .tracks
                    .get(*track_ref as usize)
                    .ok_or(ReshapeError::TrackNotExistent)?;
                match &src_track.channels {
                    Channels::Stereo(left, right) => {
                        channel_reshape.push(ReshapeSrc::Track(*left));
                        channel_reshape.push(ReshapeSrc::Track(*right));
                        new_tracks.push(TrackDescription {
                            info_v1: src_track.info_v1.clone(),
                            channels: Channels::Stereo(cur_channel_idx, cur_channel_idx + 1),
                        });
                        cur_channel_idx += 2;

                        let left_channel = brstm
                            .info
                            .channels
                            .get(*left as usize)
                            .ok_or(ReshapeError::ChannelNotExistent)?;
                        new_channels.push(left_channel.clone());
                        let right_channel = brstm
                            .info
                            .channels
                            .get(*right as usize)
                            .ok_or(ReshapeError::ChannelNotExistent)?;
                        new_channels.push(right_channel.clone());
                    }
                    Channels::Mono(_) => unreachable!(),
                }
            }
            ReshapeSrc::Empty => {
                channel_reshape.push(ReshapeSrc::Empty);
                channel_reshape.push(ReshapeSrc::Empty);
                new_tracks.push(TrackDescription {
                    info_v1: None,
                    channels: Channels::Stereo(cur_channel_idx, cur_channel_idx + 1),
                });
                cur_channel_idx += 2;

                new_channels.push(AdpcmChannelInformation::default());
                new_channels.push(AdpcmChannelInformation::default());
            }
        }
    }

    // new data
    let mut adpc_bytes =
        Vec::with_capacity(new_channels.len() * brstm.info.info.total_blocks as usize * 4);
    let mut data_bytes = Vec::with_capacity(
        new_channels.len()
            * (brstm.info.info.total_blocks.saturating_sub(1) as usize
                * brstm.info.info.blocks_size as usize
                + brstm.info.info.final_block_size_padded as usize),
    );
    for block_index in 0..brstm.info.info.total_blocks {
        let block_size = if block_index == brstm.info.info.total_blocks - 1 {
            brstm.info.info.final_block_size_padded
        } else {
            brstm.info.info.blocks_size
        };
        for channel in channel_reshape.iter() {
            match channel {
                ReshapeSrc::Empty => {
                    adpc_bytes.extend_from_slice(&[0; 4]);
                    // seems to be the best way to extend the vec with empty bytes
                    data_bytes.resize(data_bytes.len() + block_size as usize, 0);
                }
                ReshapeSrc::Track(channel_ref) => {
                    adpc_bytes.extend_from_slice(brstm.get_adpc_bytes(*channel_ref, block_index));
                    data_bytes.extend_from_slice(brstm.get_data_block(*channel_ref, block_index));
                }
            }
        }
    }
    // overwrite old values
    brstm.info.info.num_channels = new_channels.len() as u8;
    brstm.data_bytes = data_bytes;
    brstm.adpcm_bytes = adpc_bytes;
    brstm.info.tracks = new_tracks;
    brstm.info.channels = new_channels;
    Ok(())
}
