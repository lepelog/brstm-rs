use std::{env::args, fs::File};

use brstm::BrstmInformation;

pub fn try_main() -> binrw::BinResult<()> {
    let mut files = args().skip(1);
    let in_filename = files.next().expect("no in filename");
    let mut f = File::open(&in_filename)?;
    let src = BrstmInformation::from_reader(&mut f)?;
    drop(f);
    if !src.check_tracks_valid() {
        println!("{in_filename} has invalid tracks!!!");
    }
    Ok(())
}

pub fn main() {
    try_main().unwrap();
}
