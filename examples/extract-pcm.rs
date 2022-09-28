use std::{
    env::args,
    fs::File,
    io::{BufWriter, Write},
};

use brstm::brstm::ParsedBrstm;

pub fn main() {
    let filename = args().skip(1).next().unwrap();
    let mut f = File::open(&filename).unwrap();
    let reader = ParsedBrstm::parse_reader(&mut f).unwrap();
    let mut outf = BufWriter::new(File::create(format!("out.pcm")).unwrap());
    let pcm_data = reader.read_pcm(0);
    for sample in pcm_data {
        outf.write_all(&sample.to_ne_bytes()).unwrap();
    }
}
