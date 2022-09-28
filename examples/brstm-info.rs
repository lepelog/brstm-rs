use std::{
    env::args,
    fs::File,
    io::{Seek, SeekFrom},
};

use binrw::BinReaderExt;
/// get the info of a single file
///
///
use brstm::structs::{
    AdpcHeader, AdpcmChannelInformation, BrstmHeader, DataHeader, Head, Head1, Head2, Head3,
    TrackDescription,
};

pub fn process_file(filename: &String) -> binrw::BinResult<()> {
    let mut f = File::open(filename)?;
    let header = f.read_be::<BrstmHeader>()?;
    println!("{filename}");
    println!("{header:?}");
    f.seek(SeekFrom::Start(header.head_offset.into()))?;
    let head: Head = f.read_be()?;
    println!("{head:?}");
    let head_base_offset = header.head_offset + 8;
    let head1_off = head_base_offset + head.head_chunks[0].head_chunk_offset;
    f.seek(SeekFrom::Start(head1_off.into()))?;
    let head1: Head1 = f.read_be()?;
    println!("{head1:?}");
    let head2_off = head_base_offset + head.head_chunks[1].head_chunk_offset;
    f.seek(SeekFrom::Start(head2_off.into()))?;
    let head2: Head2 = f.read_be()?;
    println!("{head2:?}");
    for desc_offset in head2.track_info.iter() {
        f.seek(SeekFrom::Start(
            (head_base_offset + desc_offset.track_description_offset).into(),
        ))?;
        let track_descrption =
            f.read_be_args::<TrackDescription>((desc_offset.track_desc_type,))?;
        println!("{track_descrption:?}");
    }
    let head3_off = head_base_offset + head.head_chunks[2].head_chunk_offset;
    f.seek(SeekFrom::Start(head3_off.into()))?;
    let head3: Head3 = f.read_be()?;
    println!("{head3:?}");
    for info_offset in head3.info_offsets.iter() {
        f.seek(SeekFrom::Start(
            (head_base_offset + info_offset.offset).into(),
        ))?;
        let adpcm_info: AdpcmChannelInformation = f.read_be()?;
        println!("{adpcm_info:?}");
    }
    f.seek(SeekFrom::Start(header.adpc_offset.into()))?;
    let adpc_header: AdpcHeader = f.read_be()?;
    println!("{adpc_header:?}");
    f.seek(SeekFrom::Start(header.data_offset.into()))?;
    let data_header: DataHeader = f.read_be()?;
    println!("{data_header:?}");

    Ok(())
}
pub fn main() {
    for filename in args().skip(1) {
        match process_file(&filename) {
            Err(e) => {
                eprintln!("problem with {filename}: {e:?}");
            }
            Ok(..) => {}
        }
    }
}
