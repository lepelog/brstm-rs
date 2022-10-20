use std::{env::args, fs::File};

use brstm::BrstmInformation;

pub fn try_main() -> binrw::BinResult<()> {
    let mut files = args().skip(1);
    let in_filename = files.next().expect("no in filename");
    let out_filename = files.next().expect("no out filename");
    let mut f = File::open(in_filename)?;
    let mut src = BrstmInformation::from_reader(&mut f)?.into_with_data(&mut f)?;
    drop(f);
    src.info.fix_tracks();
    let mut outf = File::create(out_filename)?;
    src.write_brstm(&mut outf)?;
    Ok(())
}

pub fn main() {
    try_main().unwrap();
}
