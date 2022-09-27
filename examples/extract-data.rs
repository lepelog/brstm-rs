use std::{env::args, fs::File};

use brstm::brstm::ParsedBrstm;

pub fn main() {
    let filename = args().skip(1).next().unwrap();
    let mut f = File::open(&filename).unwrap();
    let reader = ParsedBrstm::parse_reader(&mut f).unwrap();
    for i in 0..reader.channel_count() {
        let mut outf = File::create(format!("out{i}.bin")).unwrap();
        reader.write_single_channel_data(i, &mut outf).unwrap();
    }
}
