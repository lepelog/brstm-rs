use std::fs::File;

use brstm::brstm::ParsedBrstm;

pub fn main() {
    let mut args = std::env::args().skip(1);
    let src = args.next().expect("source file");
    let dest = args.next().expect("dest file");
    let parsed = ParsedBrstm::parse_reader(&mut File::open(src).unwrap()).unwrap();
    parsed.write_brstm(&mut File::create(dest).unwrap()).unwrap();
}
