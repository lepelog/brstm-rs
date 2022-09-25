use binrw::binrw;

// note: HEAD chunk, an ADPC chunk and a DATA chunk. Each chunk is padded to a multiple of 0x20.

#[binrw]
#[brw(big, magic = b"RSTM")]
#[br(assert(bom == 0xFEFF), assert(header_length == 0x40))]
#[derive(Debug, Default, Clone)]
pub struct BrstmHeader {
    pub bom: u16,
    // usually 01 00
    pub version: u16,
    pub file_length: u32,
    #[br(temp)]
    #[bw(calc = 0x40)]
    pub header_length: u16,
    // usually 00 02
    pub chunk_count: u16,
    pub head_offset: u32,
    pub head_size: u32,
    pub adpc_offset: u32,
    pub adpc_size: u32,
    pub data_offset: u32,
    #[brw(pad_after = 0x18)]
    pub data_size: u32,
}

impl BrstmHeader {
    pub fn byte_len() -> u32 {
        0x40
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct HeadChunkDefs {
    #[br(temp)]
    #[bw(calc = 0x0100_0000)]
    marker: u32,
    pub head_chunk_offset: u32,
}

impl HeadChunkDefs {
    pub fn byte_len() -> u32 {
        8
    }
}

#[binrw]
#[brw(big, magic = b"HEAD")]
#[derive(Debug, Default, Clone)]
pub struct Head {
    pub head_chunk_size: u32,
    pub head_chunks: [HeadChunkDefs; 3],
}

impl Head {
    pub fn byte_len() -> u32 {
        8 + 3 * HeadChunkDefs::byte_len()
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct Head1 {
    pub codec: u8,
    pub loop_flag: u8,
    pub num_channels: u8,
    #[brw(pad_before = 1)]
    pub sample_rate: u16,
    #[brw(pad_before = 2)]
    pub loop_start: u32,
    pub total_samples: u32,
    pub audio_offset: u32,
    pub total_blocks: u32,
    pub blocks_size: u32,
    pub blocks_samples: u32,
    pub final_block_size: u32,
    pub final_block_samples: u32,
    pub final_block_size_padded: u32,
    pub adpc_samples_per_entry: u32,
    pub adpc_bytes_per_entry: u32,
}

impl Head1 {
    pub fn byte_len() -> u32 {
        52
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct Head2 {
    #[br(temp)]
    #[bw(calc = track_info.len() as u8)]
    num_tracks: u8,
    pub track_desc_type: u8,
    #[brw(pad_before = 2)]
    #[br(count = num_tracks)]
    pub track_info: Vec<TrackDescriptionOffset>,
}

impl Head2 {
    pub fn byte_len(&self) -> u32 {
        4 + self.track_info.len() as u32 * TrackDescriptionOffset::byte_len()
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct TrackDescriptionOffset {
    #[br(temp)]
    #[bw(calc = 1)]
    marker: u8,
    pub track_desc_type: u8,
    #[brw(pad_before = 2)]
    pub track_description_offset: u32,
}

impl TrackDescriptionOffset {
    pub fn byte_len() -> u32 {
        8
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct TrackDescriptionV1 {
    track_volume: u8,
    #[brw(pad_after = 6)]
    track_panning: u8,
}

#[binrw]
#[brw(big)]
#[br(import(version: u8))]
#[derive(Debug, Default, Clone)]
pub struct TrackDescription {
    #[br(if(version == 1))]
    pub info_v1: Option<TrackDescriptionV1>,
    #[br(temp)]
    #[bw(calc = track_channel.channels())]
    channels_in_track: u8,
    #[br(temp)]
    #[bw(calc = track_channel.left_channel_id())]
    left_channel_id: u8,
    #[brw(pad_after = 1)]
    #[br(temp)]
    #[bw(calc = track_channel.right_channel_id())]
    right_channel_id: u8,
    #[bw(ignore)]
    #[br(calc = if channels_in_track == 1 { TrackChannel::Mono(left_channel_id) } else { TrackChannel::Stereo(left_channel_id, right_channel_id) })]
    pub track_channel: TrackChannel,
}

impl TrackDescription {
    pub fn get_version(&self) -> u8 {
        match self.info_v1 {
            Some(..) => 1,
            None => 0,
        }
    }

    pub fn byte_len(&self) -> u32 {
        match self.info_v1 {
            Some(..) => 12,
            None => 4,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TrackChannel {
    Mono(u8),
    Stereo(u8, u8),
}

impl Default for TrackChannel {
    fn default() -> Self {
        Self::Mono(0)
    }
}

impl TrackChannel {
    pub fn channels(&self) -> u8 {
        match self {
            Self::Mono(..) => 1,
            Self::Stereo(..) => 2,
        }
    }

    pub fn left_channel_id(&self) -> u8 {
        match self {
            Self::Mono(c) => *c,
            Self::Stereo(c, _) => *c,
        }
    }

    pub fn right_channel_id(&self) -> u8 {
        match self {
            Self::Mono(_) => 0,
            Self::Stereo(_, c) => *c,
        }
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct Head3 {
    #[br(temp)]
    #[bw(calc = info_offsets.len() as u8)]
    channel_count: u8,
    #[brw(pad_before = 3)]
    #[br(count = channel_count)]
    pub info_offsets: Vec<Head3ChannelInfoOffset>,
}

impl Head3 {
    pub fn byte_len(&self) -> u32 {
        4 + self.info_offsets.len() as u32 * Head3ChannelInfoOffset::byte_len()
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct Head3ChannelInfoOffset {
    #[br(temp)]
    #[bw(calc = 0x0100_0000)]
    marker: u32,
    pub offset: u32,
}

impl Head3ChannelInfoOffset {
    pub fn byte_len() -> u32 {
        8
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct AdpcmChannelInformation {
    #[br(temp)]
    #[bw(calc = 0x0100_0000)]
    marker: u32,
    // points to the data directly after this field
    channel_adpcm_coefficients_offset: u32,
    adpcm_coefficients: [i16; 16],
    // always zero
    gain: i16,
    initial_predictor: i16,
    history_sample1: i16,
    history_sample2: i16,
    loop_predictor: i16,
    loop_history_sample1: i16,
    #[brw(pad_after = 2)]
    loop_history_sample2: i16,
}

impl AdpcmChannelInformation {
    pub fn byte_len() -> u32 {
        56
    }
}

#[cfg(test)]
mod test {
    use std::io::{Cursor, Write};

    use binrw::{BinWriterExt, BinRead, BinWrite};

    use crate::structs::{Head, Head1, Head2, TrackDescriptionOffset, Head3, Head3ChannelInfoOffset, AdpcmChannelInformation, BrstmHeader};

    #[test]
    pub fn check_byte_lens() {
        let mut buf = Vec::new();

        let header = BrstmHeader::default();
        Cursor::new(&mut buf).write_be(&header).unwrap();
        assert_eq!(BrstmHeader::byte_len() as usize, buf.len());

        buf.clear();
        let head = Head::default();
        Cursor::new(&mut buf).write_be(&head).unwrap();
        assert_eq!(Head::byte_len() as usize, buf.len());

        buf.clear();
        let head1 = Head1::default();
        Cursor::new(&mut buf).write_be(&head1).unwrap();
        assert_eq!(Head1::byte_len() as usize, buf.len());

        buf.clear();
        let mut head2 = Head2::default();
        Cursor::new(&mut buf).write_be(&head2).unwrap();
        assert_eq!(head2.byte_len() as usize, buf.len());

        buf.clear();
        head2.track_info.push(TrackDescriptionOffset::default());
        head2.track_info.push(TrackDescriptionOffset::default());
        Cursor::new(&mut buf).write_be(&head2).unwrap();
        assert_eq!(head2.byte_len() as usize, buf.len());

        buf.clear();
        let mut head3 = Head3::default();
        Cursor::new(&mut buf).write_be(&head3).unwrap();
        assert_eq!(head3.byte_len() as usize, buf.len());

        buf.clear();
        head3.info_offsets.push(Head3ChannelInfoOffset::default());
        head3.info_offsets.push(Head3ChannelInfoOffset::default());
        head3.info_offsets.push(Head3ChannelInfoOffset::default());
        Cursor::new(&mut buf).write_be(&head3).unwrap();
        assert_eq!(head3.byte_len() as usize, buf.len());

        buf.clear();
        let channel_info = AdpcmChannelInformation::default();
        Cursor::new(&mut buf).write_be(&channel_info).unwrap();
        assert_eq!(AdpcmChannelInformation::byte_len() as usize, buf.len());
    }
}
