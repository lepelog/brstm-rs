use std::io::{self, Read, Seek, SeekFrom, Write};

use binrw::{BinReaderExt, BinResult, BinWriterExt};

use crate::structs::{
    AdpcHeader, AdpcmChannelInformation, BrstmHeader, DataHeader, Head, Head1, Head2, Head3,
    HeadChunkDefs, TrackChannel, TrackDescription, TrackDescriptionOffset, TrackDescriptionV1, Head3ChannelInfoOffset,
};

fn align_next_32(off: u32) -> u32 {
    (off + 0x1F) & !0x1F
}

#[derive(Clone)]
pub struct TrackChannelInfo {
    pub channel_idx: u8,
    pub adpcm_info: AdpcmChannelInformation,
}

#[derive(Clone)]
pub enum Track {
    Mono {
        v1_info: Option<TrackDescriptionV1>,
        channel: TrackChannelInfo,
    },
    Stereo {
        v1_info: Option<TrackDescriptionV1>,
        left: TrackChannelInfo,
        right: TrackChannelInfo,
    },
}

impl Track {
    pub fn channels(&self) -> u8 {
        match self {
            Track::Mono { .. } => 1,
            Track::Stereo { .. } => 2,
        }
    }

    pub fn v1_info(&self) -> &Option<TrackDescriptionV1> {
        match self {
            Track::Mono { v1_info, .. } => v1_info,
            Track::Stereo { v1_info, .. } => v1_info,
        }
    }

    pub fn track_description(&self, add_v1: bool) -> TrackDescription {
        // returns a default v1 info if any uses it
        let default_v1 = || {
            if add_v1 {
                Some(TrackDescriptionV1 {
                    track_panning: 64,
                    track_volume: 127
                })
            } else {
                None
            }
        };
        match self {
            Track::Mono { v1_info, channel } => {
                TrackDescription {
                    info_v1: v1_info.clone().or_else(default_v1),
                    track_channel: TrackChannel::Mono(channel.channel_idx),
                }
            },
            Track::Stereo { v1_info, left, right } => {
                TrackDescription {
                    info_v1: v1_info.clone().or_else(default_v1),
                    track_channel: TrackChannel::Stereo(left.channel_idx, right.channel_idx),
                }
            },
        }
    }
}

#[derive(Clone)]
pub struct ParsedBrstm {
    pub info: Head1,
    pub tracks: Vec<Track>,
    // does not include the header
    pub adpcm_section: Vec<u8>,
    pub data_section: Vec<u8>, // adpcm_offset: u32,
                               // adpcm_size: u32,
                               // data_offset: u32,
                               // data_size: u32,
}

impl ParsedBrstm {
    pub fn parse_reader<RS: Read + Seek>(f: &mut RS) -> BinResult<Self> {
        let header = f.read_be::<BrstmHeader>()?;
        f.seek(SeekFrom::Start(header.head_offset.into()))?;
        let head: Head = f.read_be()?;
        let head_base_offset = header.head_offset + 8;
        let head1_off = head_base_offset + head.head_chunks[0].head_chunk_offset;
        f.seek(SeekFrom::Start(head1_off.into()))?;
        let head1: Head1 = f.read_be()?;
        let head2_off = head_base_offset + head.head_chunks[1].head_chunk_offset;
        f.seek(SeekFrom::Start(head2_off.into()))?;
        let head2: Head2 = f.read_be()?;
        let head3_off = head_base_offset + head.head_chunks[2].head_chunk_offset;
        f.seek(SeekFrom::Start(head3_off.into()))?;
        let head3: Head3 = f.read_be()?;
        let head2_tracks = head2.track_info.len();
        let head3_tracks = head3.info_offsets.len();
        // TODO check all
        let _is_stereo = if head2_tracks == head3_tracks {
            false
        } else if head2_tracks * 2 == head3_tracks {
            true
        } else {
            return Err(binrw::Error::AssertFail {
                pos: 0,
                message: format!(
                    "bad track counts, neither mono nor stereo: {head2_tracks} vs {head3_tracks}"
                ),
            });
        };
        let mut tracks = Vec::with_capacity(head2_tracks);
        let mut channel_info = Vec::with_capacity(head3_tracks);
        for track_adpcm_off in head3.info_offsets.iter() {
            f.seek(SeekFrom::Start(
                (head_base_offset + track_adpcm_off.offset).into(),
            ))?;
            let adpcm_info: AdpcmChannelInformation = f.read_be()?;
            channel_info.push(adpcm_info);
        }
        for (idx, track_desc_off) in head2.track_info.iter().enumerate() {
            if track_desc_off.track_desc_type != head2.track_desc_type {
                return Err(binrw::Error::AssertFail {
                    pos: 0,
                    message: format!(
                        "Differing track description type for channel {idx}: {} vs {}",
                        track_desc_off.track_desc_type, head2.track_desc_type
                    ),
                });
            }
            f.seek(SeekFrom::Start(
                (head_base_offset + track_desc_off.track_description_offset).into(),
            ))?;
            let track_descrption =
                f.read_be_args::<TrackDescription>((track_desc_off.track_desc_type,))?;
            let map_ch = |ch| TrackChannelInfo {
                channel_idx: ch,
                adpcm_info: channel_info[ch as usize].clone(),
            };
            let track = match track_descrption.track_channel {
                // TODO: catch index OoB
                TrackChannel::Mono(ch) => Track::Mono {
                    v1_info: track_descrption.info_v1.clone(),
                    channel: map_ch(ch),
                },
                TrackChannel::Stereo(left, right) => Track::Stereo {
                    v1_info: track_descrption.info_v1.clone(),
                    left: map_ch(left),
                    right: map_ch(right),
                },
            };
            tracks.push(track);
        }
        let mut adpcm_section = vec![0; header.adpc_size as usize - 8];
        f.seek(SeekFrom::Start((header.adpc_offset + 8).into()))?;
        f.read_exact(&mut adpcm_section)?;
        let mut data_section = vec![0; header.data_size as usize - 0x20];
        f.seek(SeekFrom::Start((header.data_offset + 0x20).into()))?;
        f.read_exact(&mut data_section)?;
        Ok(ParsedBrstm {
            info: head1,
            tracks,
            adpcm_section,
            data_section,
        })
    }

    pub fn write_brstm<WS: Write + Seek>(&self, ws: &mut WS) -> binrw::BinResult<()> {
        ws.seek(SeekFrom::Start(0))?;
        let channels_per_track = self.tracks[0].channels();
        let channel_count = self.tracks.len() as u32 * channels_per_track as u32;
        let any_has_v1 = self.tracks.iter().any(|t| t.v1_info().is_some());
        let track_desc_bytes = if any_has_v1 { 12 } else { 4 };
        // sanity check
        for track in self.tracks.iter().skip(1) {
            if channels_per_track != track.channels() {
                return Err(binrw::Error::AssertFail {
                    pos: 0,
                    message: "Different channel sizes per track!".into(),
                });
            }
        }
        // first, calculate all offsets
        let head_header_off = align_next_32(BrstmHeader::byte_len());
        let head1_off = head_header_off + Head::byte_len();
        let head2_off = head1_off + Head1::byte_len();
        let track_infos_off = head2_off + Head2::byte_len(self.tracks.len() as u32);
        let head3_off = track_infos_off + self.tracks.len() as u32 * track_desc_bytes;
        let channel_infos_off = head3_off + Head3::byte_len(channel_count);
        let adpcm_section_off = align_next_32(
            channel_infos_off + AdpcmChannelInformation::byte_len() * channel_count
        );
        let data_section_off = align_next_32(
            adpcm_section_off + self.adpcm_section.len() as u32 + 8
        );
        let file_length = align_next_32(
            data_section_off + self.data_section.len() as u32 + 0x20
        );
        let header = BrstmHeader {
            file_length,
            head_offset: head_header_off,
            head_size: adpcm_section_off - head_header_off,
            adpc_offset: adpcm_section_off,
            // plus header
            adpc_size: self.adpcm_section.len() as u32 + 8,
            data_offset: data_section_off,
            // plus header
            data_size: self.data_section.len() as u32 + 0x20,
        };
        ws.seek(SeekFrom::Start(0))?;
        ws.write_be(&header)?;

        let head_header = Head {
            head_chunk_size: adpcm_section_off - head_header_off,
            head_chunks: [
                // everything is relative to the HEAD section start + 8
                HeadChunkDefs { head_chunk_offset: head1_off - head_header_off - 8 },
                HeadChunkDefs { head_chunk_offset: head2_off - head_header_off - 8 },
                HeadChunkDefs { head_chunk_offset: head3_off - head_header_off - 8 },
            ],
        };
        ws.seek(SeekFrom::Start(head_header_off.into()))?;
        ws.write_be(&head_header)?;

        let head1 = Head1 {
            audio_offset: data_section_off + 0x20,
            num_channels: channel_count as u8,
            ..self.info
            // TODO: also need to fix total samples and blocks?
        };
        ws.seek(SeekFrom::Start(head1_off.into()))?;
        ws.write_be(&head1)?;

        let mut track_desc_offs = Vec::with_capacity(self.tracks.len());
        let track_desc_type = if any_has_v1 { 1 } else { 0 };
        ws.seek(SeekFrom::Start(track_infos_off.into()))?;
        for track in self.tracks.iter() {
            let off = ws.stream_position()? as u32;
            track_desc_offs.push(TrackDescriptionOffset { track_desc_type, track_description_offset: off - head_header_off - 8 });
            let track_desc = track.track_description(any_has_v1);
            ws.write_be(&track_desc)?;
        }

        let head2 = Head2 {
            track_desc_type,
            track_info: track_desc_offs
        };
        ws.seek(SeekFrom::Start(head2_off.into()))?;
        ws.write_be(&head2)?;

        let mut channel_info_offs = Vec::with_capacity(channel_count as usize);
        ws.seek(SeekFrom::Start(channel_infos_off.into()))?;
        let mut push_channel_info = |channel: &TrackChannelInfo| -> binrw::BinResult<()> {
            let off = ws.stream_position()? as u32;
            channel_info_offs.push(Head3ChannelInfoOffset { offset: off - head_header_off - 8 });
            ws.write_be(&AdpcmChannelInformation {
                // offset to coefficients that are right after the field
                // still relative to start of DATA, but 8 bytes into this sub struct
                channel_adpcm_coefficients_offset: off - head_header_off - 8 + 8,
                ..channel.adpcm_info
            })?;
            Ok(())
        };
        for track in self.tracks.iter() {
            match track {
                Track::Mono { channel, .. } => {
                    push_channel_info(channel)?;
                },
                Track::Stereo { left, right, .. } => {
                    push_channel_info(left)?;
                    push_channel_info(right)?;
                },
            }
        }

        let head3 = Head3 {
            info_offsets: channel_info_offs,
        };
        ws.seek(SeekFrom::Start(head3_off.into()))?;
        ws.write_be(&head3)?;

        let adpcm_header = AdpcHeader {
            data_len: self.adpcm_section.len() as u32 + 8
        };
        ws.seek(SeekFrom::Start(adpcm_section_off.into()))?;
        ws.write_be(&adpcm_header)?;
        ws.write_all(&self.adpcm_section)?;

        let data_header = DataHeader {
            data_len: self.data_section.len() as u32 + 0x20
        };
        ws.seek(SeekFrom::Start(data_section_off.into()))?;
        ws.write_be(&data_header)?;
        ws.write_all(&self.data_section)?;
        ws.flush()?;
        Ok(())
    }

    pub fn write_single_channel_data<W: Write>(&self, channel: u8, w: &mut W) -> io::Result<()> {
        let mut current_offset = 0;
        for i in 0..self.info.total_blocks {
            let (block_size, block_size_padded) = if i == (self.info.total_blocks - 1) {
                (
                    self.info.final_block_size,
                    self.info.final_block_size_padded,
                )
            } else {
                // all but the final block have no padding
                (self.info.blocks_size, self.info.blocks_size)
            };
            // skip over previous channels
            current_offset += channel as u32 * block_size;
            w.write_all(&self.data_section[current_offset as usize..][..block_size as usize])?;
            // skip over following channels
            current_offset += (self.info.num_channels - channel - 1) as u32 * block_size_padded;
        }
        Ok(())
    }

    pub fn channel_count(&self) -> u8 {
        self.info.num_channels
    }
}
