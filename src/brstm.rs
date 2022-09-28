use std::io::{self, Read, Seek, SeekFrom, Write};

use binrw::{BinReaderExt, BinResult, BinWriterExt};

use crate::structs::{
    AdpcHeader, AdpcmChannelInformation, BrstmHeader, DataHeader, Head, Head1, Head2, Head3,
    Head3ChannelInfoOffset, HeadChunkDefs, TrackChannel, TrackDescription, TrackDescriptionOffset,
    TrackDescriptionV1,
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
pub struct ParsedBrstm {
    pub info: Head1,
    pub tracks: Vec<TrackDescription>,
    pub channels: Vec<AdpcmChannelInformation>,
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
        let mut tracks = Vec::with_capacity(head2_tracks);
        let mut channels = Vec::with_capacity(head3_tracks);
        for track_adpcm_off in head3.info_offsets.iter() {
            f.seek(SeekFrom::Start(
                (head_base_offset + track_adpcm_off.offset).into(),
            ))?;
            let adpcm_info: AdpcmChannelInformation = f.read_be()?;
            channels.push(adpcm_info);
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
            let track = f.read_be_args::<TrackDescription>((track_desc_off.track_desc_type,))?;

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
            channels,
            adpcm_section,
            data_section,
        })
    }

    pub fn write_brstm<WS: Write + Seek>(&self, ws: &mut WS) -> binrw::BinResult<()> {
        ws.seek(SeekFrom::Start(0))?;
        let channels_per_track = self.tracks[0].track_channel.channels();
        let channel_count = self.channels.len() as u32;
        let any_has_v1 = self.tracks.iter().any(|t| t.get_version() == 1);
        let track_desc_bytes = if any_has_v1 { 12 } else { 4 };
        // sanity check
        // for track in self.tracks.iter().skip(1) {
        //     if channels_per_track != track.track_channel.channels() {
        //         return Err(binrw::Error::AssertFail {
        //             pos: 0,
        //             message: "Different channel sizes per track!".into(),
        //         });
        //     }
        // }
        // first, calculate all offsets
        let head_header_off = align_next_32(BrstmHeader::byte_len());
        let head1_off = head_header_off + Head::byte_len();
        let head2_off = head1_off + Head1::byte_len();
        let track_infos_off = head2_off + Head2::byte_len(self.tracks.len() as u32);
        let head3_off = track_infos_off + self.tracks.len() as u32 * track_desc_bytes;
        let channel_infos_off = head3_off + Head3::byte_len(channel_count);
        let adpcm_section_off =
            align_next_32(channel_infos_off + AdpcmChannelInformation::byte_len() * channel_count);
        let data_section_off =
            align_next_32(adpcm_section_off + self.adpcm_section.len() as u32 + 8);
        let file_length = align_next_32(data_section_off + self.data_section.len() as u32 + 0x20);
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
                HeadChunkDefs {
                    head_chunk_offset: head1_off - head_header_off - 8,
                },
                HeadChunkDefs {
                    head_chunk_offset: head2_off - head_header_off - 8,
                },
                HeadChunkDefs {
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

        let mut track_desc_offs = Vec::with_capacity(self.tracks.len());
        let track_desc_type = if any_has_v1 { 1 } else { 0 };
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
            track_desc_offs.push(TrackDescriptionOffset {
                track_desc_type,
                track_description_offset: off - head_header_off - 8,
            });
            let info_v1 = track.info_v1.clone().or_else(default_v1);
            ws.write_be(&TrackDescription {
                info_v1,
                ..track.clone()
            })?;
        }

        let head2 = Head2 {
            track_desc_type,
            track_info: track_desc_offs,
        };
        ws.seek(SeekFrom::Start(head2_off.into()))?;
        ws.write_be(&head2)?;

        let mut channel_info_offs = Vec::with_capacity(channel_count as usize);
        ws.seek(SeekFrom::Start(channel_infos_off.into()))?;
        for channel in self.channels.iter() {
            let off = ws.stream_position()? as u32;
            channel_info_offs.push(Head3ChannelInfoOffset {
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
            data_len: self.adpcm_section.len() as u32 + 8,
        };
        ws.seek(SeekFrom::Start(adpcm_section_off.into()))?;
        ws.write_be(&adpcm_header)?;
        ws.write_all(&self.adpcm_section)?;

        let data_header = DataHeader {
            data_len: self.data_section.len() as u32 + 0x20,
        };
        ws.seek(SeekFrom::Start(data_section_off.into()))?;
        ws.write_be(&data_header)?;
        ws.write_all(&self.data_section)?;
        ws.flush()?;
        Ok(())
    }

    pub fn make_2_track(&mut self) {
        assert!(self.tracks.len() == 1);
        let new_chan_l = self.channels[0].clone();
        let new_chan_r = self.channels[1].clone();
        self.channels.push(new_chan_l);
        self.channels.push(new_chan_r);
        let mut new = self.tracks[0].clone();
        new.track_channel = TrackChannel::Stereo(2, 3);
        self.tracks.push(new);
        // duplicate all the data
        let mut adpc_dupl = Vec::with_capacity(self.adpcm_section.len() * 2);
        let mut offset = 0;
        for _ in 0..self.info.total_blocks {
            adpc_dupl.extend_from_slice(&self.adpcm_section[offset..][..8]);
            adpc_dupl.extend_from_slice(&self.adpcm_section[offset..][..8]);
            // 2 samples for 2 channels with 2 bytes
            offset += 8;
        }
        while (adpc_dupl.len() + 8) % 32 != 0 {
            adpc_dupl.push(0);
        }
        // assert_eq!(offset, self.adpcm_section.len());
        // assert_eq!(adpc_dupl.len(), self.adpcm_section.len() * 2);
        let mut data_dupl = Vec::with_capacity(self.data_section.len() * 2);
        let mut offset = 0;
        for _ in 0..self.info.total_blocks - 1 {
            data_dupl.extend_from_slice(
                &self.data_section[offset..][..self.info.blocks_size as usize * 2],
            );
            data_dupl.extend_from_slice(
                &self.data_section[offset..][..self.info.blocks_size as usize * 2],
            );
            offset += self.info.blocks_size as usize * 2;
        }
        // last block
        data_dupl.extend_from_slice(
            &self.data_section[offset..][..self.info.final_block_size_padded as usize * 2],
        );
        data_dupl.extend_from_slice(
            &self.data_section[offset..][..self.info.final_block_size_padded as usize * 2],
        );
        offset += self.info.final_block_size_padded as usize * 2;
        assert_eq!(offset, self.data_section.len());
        assert_eq!(data_dupl.len(), self.data_section.len() * 2);
        self.adpcm_section = adpc_dupl;
        self.data_section = data_dupl;
    }

    pub fn make_2_track_silence(&mut self) {
        assert!(self.tracks.len() == 1);
        let new_chan_l = self.channels[0].clone();
        let new_chan_r = self.channels[1].clone();
        self.channels.push(new_chan_l);
        self.channels.push(new_chan_r);
        let mut new = self.tracks[0].clone();
        new.track_channel = TrackChannel::Stereo(2, 3);
        self.tracks.push(new);
        // duplicate all the data
        let mut adpc_dupl = Vec::with_capacity(self.adpcm_section.len() * 2);
        let mut offset = 0;
        let zero_sample = [0; 8];
        for _ in 0..self.info.total_blocks {
            adpc_dupl.extend_from_slice(&self.adpcm_section[offset..][..8]);
            adpc_dupl.extend_from_slice(&zero_sample);
            // 2 samples for 2 channels with 2 bytes
            offset += 8;
        }
        while (adpc_dupl.len() + 8) % 32 != 0 {
            adpc_dupl.push(0);
        }
        // assert_eq!(offset, self.adpcm_section.len());
        // assert_eq!(adpc_dupl.len(), self.adpcm_section.len() * 2);
        let mut data_dupl = Vec::with_capacity(self.data_section.len() * 2);
        let zero_block = vec![0; self.info.blocks_size as usize * 2];
        let mut offset = 0;
        for _ in 0..self.info.total_blocks - 1 {
            data_dupl.extend_from_slice(
                &self.data_section[offset..][..self.info.blocks_size as usize * 2],
            );
            data_dupl.extend_from_slice(&zero_block);
            offset += self.info.blocks_size as usize * 2;
        }
        // last block
        data_dupl.extend_from_slice(
            &self.data_section[offset..][..self.info.final_block_size_padded as usize * 2],
        );
        data_dupl.extend_from_slice(&zero_block[..self.info.final_block_size_padded as usize * 2]);
        offset += self.info.final_block_size_padded as usize * 2;
        assert_eq!(offset, self.data_section.len());
        assert_eq!(data_dupl.len(), self.data_section.len() * 2);
        self.adpcm_section = adpc_dupl;
        self.data_section = data_dupl;
    }

    pub fn swap_tracks(&mut self) {
        assert_eq!(self.tracks.len(), 1);
        self.tracks.push(self.tracks[0].clone());
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
        self.channels.len() as u8
    }

    pub fn read_pcm(&self, channel: u8) -> Vec<i16> {
        let mut result = Vec::new();
        let coeffs = &self.channels[channel as usize].adpcm_coefficients;
        assert_eq!(4, self.info.adpc_bytes_per_entry);
        // decode single block
        //
        let mut adpc_offset = 0;
        let mut data_offset = 0;
        let read_yn = |offset: &mut usize| {
            let yn1 = i16::from_be_bytes(self.adpcm_section[*offset..][..2].try_into().unwrap());
            let yn2 =
                i16::from_be_bytes(self.adpcm_section[*offset + 2..][..2].try_into().unwrap());
            *offset += 4;
            (yn1, yn2)
        };
        for block_index in 0..self.info.total_blocks {
            // skip other channels
            adpc_offset =
                block_index as usize * 4 * self.info.num_channels as usize + channel as usize * 4;
            // get yn values
            let (yn1, yn2) = read_yn(&mut adpc_offset);
            // skip over other channels
            // adpc_offset += (self.channels.len() - channel as usize - 1) * 4;

            let (block_size_padded, sample_count) = if block_index == self.info.total_blocks - 1 {
                (
                    self.info.final_block_size_padded,
                    self.info.final_block_samples,
                )
            } else {
                (self.info.blocks_size, self.info.blocks_samples)
            };
            // skip over other data channels
            // data_offset = if block_index == self.info.total_blocks - 1 {
            //     block_index as usize * self.info.num_channels as usize * self.info.blocks_size as usize + channel as usize * self.info.final_block_size_padded as usize
            // } else {
            //     (block_index as usize * self.info.num_channels as usize + channel as usize) * self.info.blocks_size as usize
            // };
            data_offset += block_size_padded as usize * channel as usize;
            do_decode(
                &self.data_section[data_offset..],
                sample_count,
                yn1,
                yn2,
                coeffs,
                &mut result,
            );
            // skip over other data channels
            data_offset += block_size_padded as usize * (self.channels.len() - channel as usize);
            println!("{data_offset}, {adpc_offset}");
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
