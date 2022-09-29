use std::{env::args, fs::File};

use brstm::brstm::BrstmInformation;

pub fn main() {
    let mut name_to_duration = Vec::new();
    for filename in args().skip(1) {
        let read = BrstmInformation::from_reader(&mut File::open(&filename).unwrap()).unwrap();
        let name = filename.split_terminator('/').last().unwrap();
        if read.info.loop_flag == 0 {
            name_to_duration.push((name.to_string(), read.info.total_samples));
        }
    }
    name_to_duration.sort_unstable_by_key(|(_, count)| *count);
    for (name, count) in name_to_duration.iter() {
        println!("{name}: {count}");
    }
}
