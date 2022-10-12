use std::io::{self, Read, Seek, SeekFrom, Write};

use binrw::{BinReaderExt, BinResult, BinWriterExt};

use crate::structs::{
    AdpcHeader, AdpcmChannelInformation, BrstmHeader, ChannelInfoOffset, Channels, DataHeader,
    Head1, Head2, Head3, HeadChunkOffsets, HeadSectionHeader, TrackDescription, TrackDescriptionV1,
    TrackInfoOffset,
};

fn align_next_32(off: u32) -> u32 {
    (off + 0x1F) & !0x1F
}

#[derive(Clone, Debug)]
pub struct BrstmInformation {
    pub info: Head1,
    pub tracks: Vec<TrackDescription>,
    pub channels: Vec<AdpcmChannelInformation>,
    // does not include the header
    adpcm_offset: u32,
    adpcm_size: u32,
    data_offset: u32,
    data_size: u32,
}

impl BrstmInformation {
    pub fn from_reader<RS: Read + Seek>(f: &mut RS) -> BinResult<Self> {
        let header = f.read_be::<BrstmHeader>()?;
        f.seek(SeekFrom::Start(header.head_offset.into()))?;
        let head: HeadSectionHeader = f.read_be()?;
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
        let head2_tracks = head2.track_info_offsets.len();
        let head3_tracks = head3.info_offsets.len();
        let mut tracks = Vec::with_capacity(head2_tracks);
        let mut channels = Vec::with_capacity(head3_tracks);
        for track_adpcm_off in head3.info_offsets.iter() {
            f.seek(SeekFrom::Start(
                (head_base_offset + track_adpcm_off.offset).into(),
            ))?;
            let adpcm_info: AdpcmChannelInformation = f.read_be()?;
            channels.push(adpcm_info);
        }
        for (idx, track_desc_off) in head2.track_info_offsets.iter().enumerate() {
            if track_desc_off.track_info_type != head2.track_info_type {
                return Err(binrw::Error::AssertFail {
                    pos: 0,
                    message: format!(
                        "Differing track description type for channel {idx}: {} vs {}",
                        track_desc_off.track_info_type, head2.track_info_type
                    ),
                });
            }
            f.seek(SeekFrom::Start(
                (head_base_offset + track_desc_off.offset).into(),
            ))?;
            let track = f.read_be_args::<TrackDescription>((track_desc_off.track_info_type,))?;

            tracks.push(track);
        }
        let mut adpcm_section = vec![0; header.adpc_size as usize - 8];
        f.seek(SeekFrom::Start((header.adpc_offset + 8).into()))?;
        f.read_exact(&mut adpcm_section)?;
        let mut data_section = vec![0; header.data_size as usize - 0x20];
        f.seek(SeekFrom::Start((header.data_offset + 0x20).into()))?;
        f.read_exact(&mut data_section)?;
        Ok(BrstmInformation {
            info: head1,
            tracks,
            channels,
            adpcm_offset: header.adpc_offset + 8,
            adpcm_size: header.adpc_size - 8,
            data_offset: header.data_offset + 0x20,
            data_size: header.data_size - 0x20,
        })
    }

    pub fn write_brstm<WS: Write + Seek>(
        &self,
        ws: &mut WS,
        adpcm_bytes: &[u8],
        data_bytes: &[u8],
    ) -> binrw::BinResult<()> {
        ws.seek(SeekFrom::Start(0))?;
        let channel_count = self.channels.len() as u32;
        let any_has_v1 = self.tracks.iter().any(|t| t.get_version() == 1);
        let track_desc_bytes = if any_has_v1 { 12 } else { 4 };

        let adpc_section_len = adpcm_bytes.len() as u32 + 8;
        let adpc_section_len_aligned = align_next_32(adpc_section_len);
        // first, calculate all offsets
        let head_header_off = align_next_32(BrstmHeader::byte_len());
        let head1_off = head_header_off + HeadSectionHeader::byte_len();
        let head2_off = head1_off + Head1::byte_len();
        let track_infos_off = head2_off + Head2::byte_len(self.tracks.len() as u32);
        let head3_off = track_infos_off + self.tracks.len() as u32 * track_desc_bytes;
        let channel_infos_off = head3_off + Head3::byte_len(channel_count);
        let adpcm_section_off =
            align_next_32(channel_infos_off + AdpcmChannelInformation::byte_len() * channel_count);
        let data_section_off = align_next_32(adpcm_section_off + adpc_section_len_aligned);
        let file_length = align_next_32(data_section_off + data_bytes.len() as u32 + 0x20);
        let header = BrstmHeader {
            file_length,
            head_offset: head_header_off,
            head_size: adpcm_section_off - head_header_off,
            adpc_offset: adpcm_section_off,
            // plus header
            adpc_size: adpc_section_len_aligned,
            data_offset: data_section_off,
            // plus header
            data_size: data_bytes.len() as u32 + 0x20,
        };
        ws.seek(SeekFrom::Start(0))?;
        ws.write_be(&header)?;

        let head_header = HeadSectionHeader {
            head_chunk_size: adpcm_section_off - head_header_off,
            head_chunks: [
                // everything is relative to the HEAD section start + 8
                HeadChunkOffsets {
                    head_chunk_offset: head1_off - head_header_off - 8,
                },
                HeadChunkOffsets {
                    head_chunk_offset: head2_off - head_header_off - 8,
                },
                HeadChunkOffsets {
                    head_chunk_offset: head3_off - head_header_off - 8,
                },
            ],
        };
        ws.seek(SeekFrom::Start(head_header_off.into()))?;
        ws.write_be(&head_header)?;

        let head1 = Head1 {
            audio_offset: data_section_off + 0x20,
            num_channels: channel_count as u8,
            ..self.info // TODO: also need to fix total samples and blocks?
        };
        ws.seek(SeekFrom::Start(head1_off.into()))?;
        ws.write_be(&head1)?;

        let mut track_info_offsets = Vec::with_capacity(self.tracks.len());
        let track_info_type = if any_has_v1 { 1 } else { 0 };
        let default_v1 = || {
            if any_has_v1 {
                Some(TrackDescriptionV1 {
                    track_panning: 64,
                    track_volume: 127,
                })
            } else {
                None
            }
        };
        ws.seek(SeekFrom::Start(track_infos_off.into()))?;
        for track in self.tracks.iter() {
            let off = ws.stream_position()? as u32;
            track_info_offsets.push(TrackInfoOffset {
                track_info_type,
                offset: off - head_header_off - 8,
            });
            let info_v1 = track.info_v1.clone().or_else(default_v1);
            ws.write_be(&TrackDescription {
                info_v1,
                ..track.clone()
            })?;
        }

        let head2 = Head2 {
            track_info_type,
            track_info_offsets,
        };
        ws.seek(SeekFrom::Start(head2_off.into()))?;
        ws.write_be(&head2)?;

        let mut channel_info_offs = Vec::with_capacity(channel_count as usize);
        ws.seek(SeekFrom::Start(channel_infos_off.into()))?;
        for channel in self.channels.iter() {
            let off = ws.stream_position()? as u32;
            channel_info_offs.push(ChannelInfoOffset {
                offset: off - head_header_off - 8,
            });
            ws.write_be(&AdpcmChannelInformation {
                // offset to coefficients that are right after the field
                // still relative to start of DATA, but 8 bytes into this sub struct
                channel_adpcm_coefficients_offset: off - head_header_off - 8 + 8,
                ..*channel
            })?;
        }

        let head3 = Head3 {
            info_offsets: channel_info_offs,
        };
        ws.seek(SeekFrom::Start(head3_off.into()))?;
        ws.write_be(&head3)?;

        let adpcm_header = AdpcHeader {
            data_len: align_next_32(adpc_section_len_aligned),
        };
        ws.seek(SeekFrom::Start(adpcm_section_off.into()))?;
        ws.write_be(&adpcm_header)?;
        ws.write_all(adpcm_bytes)?;
        // pad to next 32
        let pad = [0; 32];
        ws.write_all(&pad[..(adpc_section_len_aligned - adpc_section_len) as usize])?;

        let data_header = DataHeader {
            data_len: data_bytes.len() as u32 + 0x20,
        };
        ws.seek(SeekFrom::Start(data_section_off.into()))?;
        ws.write_be(&data_header)?;
        ws.write_all(data_bytes)?;
        ws.flush()?;
        Ok(())
    }

    pub fn channel_count(&self) -> u8 {
        self.channels.len() as u8
    }

    /// fixes songs with tracks that point to invalid channels
    /// returns if tracks had to be fixed
    pub fn fix_tracks(&mut self) -> bool {
        self.info.num_channels = self.channels.len() as u8;
        let mut made_change = false;
        self.tracks.retain(|track| {
            match &track.channels {
                Channels::Mono(channel) => {
                    if *channel >= self.info.num_channels {
                        made_change = true;
                        return false;
                    }
                }
                Channels::Stereo(left, right) => {
                    if *left >= self.info.num_channels {
                        made_change = true;
                        return false;
                    }
                    if *right >= self.info.num_channels {
                        made_change = true;
                        return false;
                    }
                }
            }
            true
        });
        made_change
    }

    pub fn into_with_data<RS: Read + Seek>(self, f: &mut RS) -> io::Result<BrstmInfoWithData> {
        let mut adpcm_bytes = vec![0; self.adpcm_size as usize];
        f.seek(SeekFrom::Start(self.adpcm_offset.into()))?;
        f.read_exact(&mut adpcm_bytes)?;
        let mut data_bytes = vec![0; self.data_size as usize];
        f.seek(SeekFrom::Start(self.data_offset.into()))?;
        f.read_exact(&mut data_bytes)?;
        Ok(BrstmInfoWithData {
            info: self,
            adpcm_bytes,
            data_bytes,
        })
    }
}

pub struct BrstmInfoWithData {
    pub info: BrstmInformation,
    pub adpcm_bytes: Vec<u8>,
    pub data_bytes: Vec<u8>,
}

impl BrstmInfoWithData {
    pub fn write_brstm<WS: Write + Seek>(&self, ws: &mut WS) -> binrw::BinResult<()> {
        self.info
            .write_brstm(ws, &self.adpcm_bytes, &self.data_bytes)
    }

    pub fn get_adpc_bytes(&self, channel: u8, block_index: u32) -> &[u8; 4] {
        let adpc_offset =
            block_index as usize * 4 * self.info.info.num_channels as usize + channel as usize * 4;

        self.adpcm_bytes[adpc_offset..][..4].try_into().unwrap()
    }

    pub fn get_adpc_values(&self, channel: u8, block_index: u32) -> (i16, i16) {
        let bytes = self.get_adpc_bytes(channel, block_index);

        (
            i16::from_be_bytes(bytes[..2].try_into().unwrap()),
            i16::from_be_bytes(bytes[2..4].try_into().unwrap()),
        )
    }

    pub fn get_data_block(&self, channel: u8, block_index: u32) -> &[u8] {
        let (data_offset, block_size) = if block_index == self.info.info.total_blocks - 1 {
            (
                block_index as usize
                    * self.info.info.num_channels as usize
                    * self.info.info.blocks_size as usize
                    + channel as usize * self.info.info.final_block_size_padded as usize,
                self.info.info.final_block_size_padded,
            )
        } else {
            (
                (block_index as usize * self.info.info.num_channels as usize + channel as usize)
                    * self.info.info.blocks_size as usize,
                self.info.info.blocks_size,
            )
        };
        &self.data_bytes[data_offset..][..block_size as usize]
    }

    pub fn get_data_block_with_samplecount(&self, channel: u8, block_index: u32) -> (&[u8], u32) {
        let (data_offset, block_size, block_samples) = if block_index
            == self.info.info.total_blocks - 1
        {
            (
                block_index as usize
                    * self.info.info.num_channels as usize
                    * self.info.info.blocks_size as usize
                    + channel as usize * self.info.info.final_block_size_padded as usize,
                self.info.info.final_block_size_padded,
                self.info.info.final_block_samples,
            )
        } else {
            (
                (block_index as usize * self.info.info.num_channels as usize + channel as usize)
                    * self.info.info.blocks_size as usize,
                self.info.info.blocks_size,
                self.info.info.blocks_samples,
            )
        };
        (
            &self.data_bytes[data_offset..][..block_size as usize],
            block_samples,
        )
    }

    pub fn get_pcm(&self, channel: u8) -> Vec<i16> {
        let mut result = Vec::new();
        let coeffs = &self.info.channels[channel as usize].adpcm_coefficients;
        assert_eq!(4, self.info.info.adpc_bytes_per_entry);
        for block_index in 0..self.info.info.total_blocks {
            // decode single block
            let (yn1, yn2) = self.get_adpc_values(channel, block_index);
            let (data, sample_count) = self.get_data_block_with_samplecount(channel, block_index);
            do_decode(data, sample_count, yn1, yn2, coeffs, &mut result);
        }

        result
    }
}

fn do_decode(
    data: &[u8],
    sample_count: u32,
    yn1: i16,
    yn2: i16,
    coeffs: &[i16; 16],
    out_buf: &mut Vec<i16>,
) {
    // see https://github.com/libertyernie/brawltools/blob/master/BrawlLib/Wii/Audio/ADPCMState.cs
    let mut data_offset = 0;
    let mut cps = 0;
    let mut cyn1 = yn1;
    let mut cyn2 = yn2;
    for sample_idx in 0..sample_count {
        if sample_idx % 14 == 0 {
            cps = data[data_offset];
            data_offset += 1;
        }
        let mut out_sample = if sample_idx % 2 == 0 {
            (data[data_offset] >> 4) as i32
        } else {
            let tmp = data[data_offset] & 0xF;
            data_offset += 1;
            tmp as i32
        };
        if out_sample >= 8 {
            out_sample -= 16;
        }
        let scale = 1 << (cps & 0xF);
        let c_index = (cps >> 4) << 1;
        out_sample = (0x400
            + ((scale * out_sample) << 11)
            + coeffs[c_index.clamp(0, 15) as usize] as i32 * cyn1 as i32
            + coeffs[(c_index + 1).clamp(0, 15) as usize] as i32 * cyn2 as i32)
            >> 11;

        cyn2 = cyn1;
        cyn1 = out_sample.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        out_buf.push(cyn1);
    }
}
