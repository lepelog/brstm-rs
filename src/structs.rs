use std::io::{Read, Seek};

use binrw::{binrw, BinReaderExt, BinResult};

// note: HEAD chunk, an ADPC chunk and a DATA chunk. Each chunk is padded to a multiple of 0x20.

#[binrw]
#[brw(big, magic = b"RSTM")]
#[br(assert(bom == 0xFEFF), assert(header_length == 0x40), assert(version == 0x0100), assert(chunk_count == 2))]
#[derive(Debug, Default, Clone)]
pub struct BrstmHeader {
    #[br(temp)]
    #[bw(calc = 0xFEFF)]
    pub bom: u16,
    // usually 01 00
    #[br(temp)]
    #[bw(calc = 0x0100)]
    pub version: u16,
    pub file_length: u32,
    #[br(temp)]
    #[bw(calc = 0x40)]
    pub header_length: u16,
    // usually 00 02
    #[br(temp)]
    #[bw(calc = 2)]
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
pub struct HeadChunkOffsets {
    #[br(temp)]
    #[bw(calc = 0x0100_0000)]
    marker: u32,
    pub head_chunk_offset: u32,
}

impl HeadChunkOffsets {
    pub fn byte_len() -> u32 {
        8
    }
}

#[binrw]
#[brw(big, magic = b"HEAD")]
#[derive(Debug, Default, Clone)]
pub struct HeadSectionHeader {
    pub head_chunk_size: u32,
    pub head_chunks: [HeadChunkOffsets; 3],
}

impl HeadSectionHeader {
    pub fn byte_len() -> u32 {
        8 + 3 * HeadChunkOffsets::byte_len()
    }
}

#[binrw]
#[brw(big)]
#[br(assert(adpc_bytes_per_entry == 4))]
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
    #[bw(calc = track_info_offsets.len() as u8)]
    num_tracks: u8,
    pub track_info_type: u8,
    #[brw(pad_before = 2)]
    #[br(count = num_tracks)]
    pub track_info_offsets: Vec<TrackInfoOffset>,
}

impl Head2 {
    pub fn byte_len(track_count: u32) -> u32 {
        4 + track_count * TrackInfoOffset::byte_len()
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct TrackInfoOffset {
    #[br(temp)]
    #[bw(calc = 1)]
    marker: u8,
    pub track_info_type: u8,
    #[brw(pad_before = 2)]
    pub offset: u32,
}

impl TrackInfoOffset {
    pub fn byte_len() -> u32 {
        8
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Clone)]
pub struct TrackDescriptionV1 {
    pub track_volume: u8,
    #[brw(pad_after = 6)]
    pub track_panning: u8,
}

impl Default for TrackDescriptionV1 {
    fn default() -> Self {
        Self {
            track_volume: 0x7F,
            track_panning: 64,
        }
    }
}

#[binrw]
#[brw(big)]
#[br(import(version: u8))]
#[derive(Debug, Default, Clone)]
pub struct TrackDescription {
    #[br(if(version == 1))]
    pub info_v1: Option<TrackDescriptionV1>,
    #[br(temp)]
    #[bw(calc = channels.channels())]
    channels_in_track: u8,
    #[br(temp)]
    #[bw(calc = channels.left_channel_id())]
    left_channel_id: u8,
    #[brw(pad_after = 1)]
    #[br(temp)]
    #[bw(calc = channels.right_channel_id())]
    right_channel_id: u8,
    #[bw(ignore)]
    #[br(calc = if channels_in_track == 1 { Channels::Mono(left_channel_id) } else { Channels::Stereo(left_channel_id, right_channel_id) })]
    pub channels: Channels,
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
pub enum Channels {
    Mono(u8),
    Stereo(u8, u8),
}

impl Default for Channels {
    fn default() -> Self {
        Self::Mono(0)
    }
}

impl Channels {
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
    pub info_offsets: Vec<ChannelInfoOffset>,
}

impl Head3 {
    pub fn byte_len(channel_count: u32) -> u32 {
        4 + channel_count * ChannelInfoOffset::byte_len()
    }
}

#[binrw]
#[brw(big)]
#[derive(Debug, Default, Clone)]
pub struct ChannelInfoOffset {
    #[br(temp)]
    #[bw(calc = 0x0100_0000)]
    marker: u32,
    pub offset: u32,
}

impl ChannelInfoOffset {
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
    pub channel_adpcm_coefficients_offset: u32,
    pub adpcm_coefficients: [i16; 16],
    // always zero
    pub gain: i16,
    pub initial_predictor: i16,
    pub history_sample1: i16,
    pub history_sample2: i16,
    pub loop_predictor: i16,
    pub loop_history_sample1: i16,
    #[brw(pad_after = 2)]
    pub loop_history_sample2: i16,
}

impl AdpcmChannelInformation {
    pub fn byte_len() -> u32 {
        56
    }
}

#[binrw]
#[brw(big, magic = b"ADPC")]
#[derive(Debug, Default, Clone)]
pub struct AdpcHeader {
    pub data_len: u32,
}

pub fn read_adpcm_section<R: Read + Seek>(r: &mut R) -> BinResult<Vec<u8>> {
    let header: AdpcHeader = r.read_be()?;
    // TODO: use ReadBuf (or whatever it turns into)
    let mut buf = vec![0; header.data_len as usize - 8];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

#[binrw]
#[brw(big, magic = b"DATA")]
#[br(assert(padding_bytes == 0x18))]
#[derive(Debug, Default, Clone)]
pub struct DataHeader {
    pub data_len: u32,
    #[brw(pad_after = 0x14)]
    #[br(temp)]
    #[bw(calc = 0x18)]
    padding_bytes: u32,
}

pub fn read_data_section<R: Read + Seek>(r: &mut R) -> BinResult<Vec<u8>> {
    let header: DataHeader = r.read_be()?;
    // TODO: use ReadBuf (or whatever it turns into)
    let mut buf = vec![0; header.data_len as usize - 0x20];
    r.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use binrw::BinWriterExt;

    use crate::structs::{
        AdpcmChannelInformation, BrstmHeader, ChannelInfoOffset, Head1, Head2, Head3,
        HeadSectionHeader, TrackInfoOffset,
    };

    #[test]
    pub fn check_byte_lens() {
        let mut buf = Vec::new();

        let header = BrstmHeader::default();
        Cursor::new(&mut buf).write_be(&header).unwrap();
        assert_eq!(BrstmHeader::byte_len() as usize, buf.len());

        buf.clear();
        let head = HeadSectionHeader::default();
        Cursor::new(&mut buf).write_be(&head).unwrap();
        assert_eq!(HeadSectionHeader::byte_len() as usize, buf.len());

        buf.clear();
        let head1 = Head1::default();
        Cursor::new(&mut buf).write_be(&head1).unwrap();
        assert_eq!(Head1::byte_len() as usize, buf.len());

        buf.clear();
        let mut head2 = Head2::default();
        Cursor::new(&mut buf).write_be(&head2).unwrap();
        assert_eq!(Head2::byte_len(0) as usize, buf.len());

        buf.clear();
        head2.track_info_offsets.push(TrackInfoOffset::default());
        head2.track_info_offsets.push(TrackInfoOffset::default());
        Cursor::new(&mut buf).write_be(&head2).unwrap();
        assert_eq!(Head2::byte_len(2) as usize, buf.len());

        buf.clear();
        let mut head3 = Head3::default();
        Cursor::new(&mut buf).write_be(&head3).unwrap();
        assert_eq!(Head3::byte_len(0) as usize, buf.len());

        buf.clear();
        head3.info_offsets.push(ChannelInfoOffset::default());
        head3.info_offsets.push(ChannelInfoOffset::default());
        head3.info_offsets.push(ChannelInfoOffset::default());
        Cursor::new(&mut buf).write_be(&head3).unwrap();
        assert_eq!(Head3::byte_len(3) as usize, buf.len());

        buf.clear();
        let channel_info = AdpcmChannelInformation::default();
        Cursor::new(&mut buf).write_be(&channel_info).unwrap();
        assert_eq!(AdpcmChannelInformation::byte_len() as usize, buf.len());
    }
}
