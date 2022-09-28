use std::{
    fs::{self},
    io::Cursor,
};

use brstm::brstm::ParsedBrstm;

pub fn main() {
    for src in std::env::args().skip(1) {
        let orig = fs::read(&src).unwrap();
        let mut dest = Vec::with_capacity(orig.len());
        println!("{src}");
        let parsed = ParsedBrstm::parse_reader(&mut Cursor::new(&orig)).unwrap();
        parsed.write_brstm(&mut Cursor::new(&mut dest)).unwrap();
        if orig != dest {
            println!("missmatch: {src}");
        }
    }
}
