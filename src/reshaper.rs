use crate::{
    brstm::BrstmInfoWithData,
    structs::{AdpcmChannelInformation, Channels, TrackDescription},
};
use thiserror::Error;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum AdditionalTrackKind {
    Normal,
    Additive,
}

#[derive(Debug, Clone)]
pub enum ReshapeTrackDef {
    Stereo { left: ReshapeSrc, right: ReshapeSrc },
    Mono { channel: ReshapeSrc },
}

#[derive(Debug, Clone)]
pub enum ReshapeSrc {
    Channel(u8),
    Empty,
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

/// assumes tracks are in a specific layout: Stereo(0,1), Stereo(2,3), etc
/// or Mono(0), Mono(1)
pub fn calc_reshape(
    original: &[AdditionalTrackKind],
    original_is_stereo: bool,
    new: &[AdditionalTrackKind],
    new_is_stereo: bool,
) -> Vec<ReshapeTrackDef> {
    let get_reshape_src_ref = |track_no: u8| {
        if new_is_stereo {
            if original_is_stereo {
                ReshapeTrackDef::Stereo {
                    left: ReshapeSrc::Channel(track_no * 2),
                    right: ReshapeSrc::Channel(track_no * 2 + 1),
                }
            } else {
                ReshapeTrackDef::Stereo {
                    left: ReshapeSrc::Channel(track_no),
                    right: ReshapeSrc::Channel(track_no),
                }
            }
        } else {
            #[allow(clippy::collapsible_else_if)]
            if original_is_stereo {
                ReshapeTrackDef::Mono {
                    channel: ReshapeSrc::Channel(track_no * 2),
                }
            } else {
                ReshapeTrackDef::Mono {
                    channel: ReshapeSrc::Channel(track_no),
                }
            }
        }
    };
    // main track always stays
    let mut result = Vec::with_capacity(new.len() + 1);
    result.push(get_reshape_src_ref(0));
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
                get_reshape_src_ref(orig_normal_tracks.next().unwrap_or(0))
            }
            AdditionalTrackKind::Additive => orig_additive_tracks
                .next()
                .map(get_reshape_src_ref)
                .unwrap_or_else(|| {
                    if new_is_stereo {
                        ReshapeTrackDef::Stereo {
                            left: ReshapeSrc::Empty,
                            right: ReshapeSrc::Empty,
                        }
                    } else {
                        ReshapeTrackDef::Mono {
                            channel: ReshapeSrc::Empty,
                        }
                    }
                }),
        };
        result.push(reshape_entry);
    }
    result
}

pub fn reshape(
    brstm: &mut BrstmInfoWithData,
    track_reshape: &[ReshapeTrackDef],
) -> Result<(), ReshapeError> {
    // first, figure out how channels need to be reshaped
    let mut channel_reshape = Vec::new();
    let mut cur_channel_idx = 0;
    let mut new_tracks = Vec::new();
    let mut new_channels = Vec::new();
    let get_info_v1 = |reshape_src: &ReshapeSrc| match reshape_src {
        ReshapeSrc::Channel(channel) => brstm
            .info
            .tracks
            .iter()
            .find(|track| track.channels.includes_channel(*channel))
            .and_then(|track| track.info_v1.clone()),
        _ => None,
    };
    let get_new_channel =
        |reshape_src: &ReshapeSrc| -> Result<AdpcmChannelInformation, ReshapeError> {
            match reshape_src {
                ReshapeSrc::Channel(channel) => brstm
                    .info
                    .channels
                    .get(*channel as usize)
                    .cloned()
                    .ok_or(ReshapeError::ChannelNotExistent),
                ReshapeSrc::Empty => Ok(AdpcmChannelInformation::default()),
            }
        };
    for track in track_reshape.iter() {
        match track {
            ReshapeTrackDef::Stereo { left, right } => {
                channel_reshape.push(left.clone());
                channel_reshape.push(right.clone());
                // figure out if the old track info belonging to the left channel
                // had v1 info, if yes include it
                new_tracks.push(TrackDescription {
                    info_v1: get_info_v1(left),
                    channels: Channels::Stereo(cur_channel_idx, cur_channel_idx + 1),
                });
                cur_channel_idx += 2;
                new_channels.push(get_new_channel(left)?);
                new_channels.push(get_new_channel(right)?);
            }
            ReshapeTrackDef::Mono { channel } => {
                channel_reshape.push(channel.clone());

                new_tracks.push(TrackDescription {
                    info_v1: get_info_v1(channel),
                    channels: Channels::Mono(cur_channel_idx),
                });
                cur_channel_idx += 1;
                new_channels.push(get_new_channel(channel)?);
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
                ReshapeSrc::Channel(channel_ref) => {
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
