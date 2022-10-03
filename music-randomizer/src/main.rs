use brstm::brstm::BrstmInformation;
use clap::Parser;
use rand::{SeedableRng, random};
use vanilla_info::VanillaInfo;
use std::{path::PathBuf, process::exit};
use rand_pcg::Pcg64;

use loader::read_music_dir_rec;
use randomizer::{execute_patches, PatchEntry};
use reshaper::AdditionalTracksType;

use crate::randomizer::randomize;

mod loader;
mod randomizer;
mod reshaper;
mod vanilla_info;

#[derive(Parser)]
pub struct Args {
    #[arg(short, long)]
    /// seed for randomization, default random
    seed: Option<u64>,
    #[arg(short, long)]
    /// the randomizer directory, current directory by default
    base_path: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    let base_path = args.base_path.unwrap_or_else(|| PathBuf::from("."));
    let vanilla_dir = {
        let mut tmp = base_path.clone();
        tmp.push("actual-extract");
        tmp.push("DATA");
        tmp.push("files");
        tmp.push("Sound");
        tmp.push("wzs");
        tmp
    };
    if !vanilla_dir.exists() {
        eprintln!("The actual-extract folder doesn't exist or doesn't have the right structure, make sure to place this program next to the rando!");
        exit(1);
    }
    let custom_dir = {
        let mut tmp = base_path.clone();
        tmp.push("custom-music");
        tmp
    };
    if !custom_dir.exists() {
        eprintln!("The custom music directory doesn't exist! Make sure it's named custom-music!");
        exit(1);
    }
    let dest_dir = {
        let mut tmp = base_path;
        tmp.push("modified-extract");
        tmp.push("DATA");
        tmp.push("files");
        tmp.push("Sound");
        tmp.push("wzs");
        tmp
    };
    if !dest_dir.exists() {
        eprintln!("The modified-extract folder doesn't exist or doesn't have the right structure, make sure to place this program next to the rando!");
        exit(1);
    }

    let mut custom_looping = Vec::new();
    let mut custom_short_nonloop = Vec::new();
    let mut custom_long_nonloop = Vec::new();
    read_music_dir_rec(&custom_dir, 5, &mut custom_looping, &mut custom_short_nonloop, &mut custom_long_nonloop).unwrap();

    let (looping_vanilla, short_vanilla, long_vanilla) = vanilla_info::load();

    let mut rng = Pcg64::seed_from_u64(args.seed.unwrap_or_else(|| random()));

    let mut do_handle = |mut custom: Vec<(PathBuf, BrstmInformation)>, vanilla: &Vec<VanillaInfo>| {
        println!(
            "custom music: {}, vanilla: {}",
            custom.len(),
            vanilla.len()
        );

        custom.sort_by(|(p1, _), (p2, _)| p1.file_name().unwrap().cmp(p2.file_name().unwrap()));

        let custom_mapped = custom.into_iter().map(|(path, info)| {
            (path, AdditionalTracksType::from(&info))
        }).collect();

        let patches = randomize(&mut rng, vanilla, &custom_mapped, &vanilla_dir);
        execute_patches(&patches, &dest_dir).unwrap();
    };

    do_handle(custom_looping, &looping_vanilla);
    do_handle(custom_short_nonloop, &short_vanilla);
    do_handle(custom_long_nonloop, &long_vanilla);
}
