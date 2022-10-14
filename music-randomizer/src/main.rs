use clap::Parser;
use env_logger::Env;
use log::{error, info};
use rand::{random, SeedableRng};
use rand_pcg::Pcg64;
use std::{path::PathBuf, process::exit};

use loader::read_all_music_packs;
use randomizer::execute_patches;

use crate::randomizer::randomize;

mod loader;
mod randomizer;
mod vanilla_info;

#[derive(Parser)]
/// Music randomizer for Skyward Sword
pub struct Args {
    #[arg(short, long)]
    /// Seed for randomization, default random
    seed: Option<u64>,
    #[arg(short = 'p', long)]
    /// The randomizer directory, current directory by default
    base_path: Option<PathBuf>,
    #[arg(short, long)]
    /// Ignore specific replacements and shuffle all
    random: bool,
    #[arg(short, long)]
    /// Allow duplicating songs instead of using songs from vanilla
    limit_vanilla: bool,
}

fn main() {
    let env = Env::new().default_filter_or("info");
    env_logger::init_from_env(env);
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
        error!("The actual-extract folder doesn't exist or doesn't have the right structure, make sure to place this program next to the rando!");
        exit(1);
    }
    let custom_dir = {
        let mut tmp = base_path.clone();
        tmp.push("custom-music");
        tmp
    };
    if !custom_dir.exists() {
        error!("The custom music directory doesn't exist! Make sure it's named custom-music!");
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
        error!("The modified-extract folder doesn't exist or doesn't have the right structure, make sure to place this program next to the rando!");
        exit(1);
    }

    let seed = args.seed.unwrap_or_else(random);
    info!("using seed {seed}");
    let mut rng = Pcg64::seed_from_u64(args.seed.unwrap_or_else(random));

    let music_packs = read_all_music_packs(&custom_dir).unwrap();
    let vanilla_songs = vanilla_info::load();

    let patches = randomize(
        &mut rng,
        vanilla_songs,
        music_packs,
        args.random,
        args.limit_vanilla,
    );
    execute_patches(patches, &vanilla_dir, &dest_dir).unwrap();
}
