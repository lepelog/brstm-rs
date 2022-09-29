use std::{
    fs::{self},
    io::Cursor,
};

use brstm::brstm::BrstmInformation;

pub fn main() {
    for src in std::env::args().skip(1) {
        let orig = fs::read(&src).unwrap();
        let mut dest = Vec::with_capacity(orig.len());
        println!("{src}");
        let mut cursor = Cursor::new(&orig);
        let parsed = BrstmInformation::from_reader(&mut cursor).unwrap();
        let data_parsed = parsed.into_with_data(&mut cursor).unwrap();
        data_parsed.write_brstm(&mut Cursor::new(&mut dest)).unwrap();
        if orig != dest {
            println!("missmatch: {src}");
        }
    }
}
