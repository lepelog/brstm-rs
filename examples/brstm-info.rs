use std::{fs::File, env::args, io::{Seek, SeekFrom}};

use binrw::BinReaderExt;
/// get the info of a single file
/// 
/// 

use brstm::structs::{BrstmHeader, Head, Head1, Head2};

pub fn process_file(filename: &String) -> binrw::BinResult<()> {
    let mut f = File::open(filename)?;
    let header = f.read_be::<BrstmHeader>()?;
    println!("{filename}");
    println!("{header:?}");
    f.seek(SeekFrom::Start(header.head_offset.into()))?;
    let head: Head = f.read_be()?;
    println!("{head:?}");
    let head1_off = header.head_offset + head.head_chunks[0].head_chunk_offset + 8;
    f.seek(SeekFrom::Start(head1_off.into()))?;
    let head1: Head1 = f.read_be()?;
    println!("{head1:?}");
    let head2_off = header.head_offset + head.head_chunks[1].head_chunk_offset + 8;
    f.seek(SeekFrom::Start(head2_off.into()))?;
    let head1: Head2 = f.read_be()?;
    println!("{head1:?}");

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
