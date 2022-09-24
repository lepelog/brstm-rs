use binrw::binrw;

#[binrw]
#[brw(big, magic = b"RSTM")]
#[br(assert(bom == 0xFEFF), assert(header_length == 0x40))]
#[derive(Debug)]
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
    #[brw(pad_after = 18)]
    pub data_size: u32,
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
pub struct HeadChunkDefs {
    #[br(temp)]
    #[bw(calc = 0x0100_0000)]
    marker: u32,
    pub head_chunk_offset: u32,
}

#[binrw]
#[brw(big, magic = b"HEAD")]
#[derive(Debug)]
pub struct Head {
    pub head_chunk_size: u32,
    pub head_chunks: [HeadChunkDefs; 3],
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
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

#[binrw]
#[brw(big)]
#[derive(Debug)]
pub struct Head2 {
    #[br(temp)]
    #[bw(calc = track_info.len() as u8)]
    num_tracks: u8,
    pub track_desc_type: u8,
    #[brw(pad_before = 2)]
    #[br(count = num_tracks)]
    pub track_info: Vec<Head2TrackInfo>,
}

#[binrw]
#[brw(big)]
#[derive(Debug)]
pub struct Head2TrackInfo {
    #[br(temp)]
    #[bw(calc = 1)]
    marker: u8,
    pub track_desc_type: u8,
    #[brw(pad_before = 2)]
    pub track_offset: u32,
}

