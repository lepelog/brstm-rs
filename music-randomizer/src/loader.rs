use std::{
    collections::HashMap,
    ffi::OsString,
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use brstm::brstm::BrstmInformation;

use crate::reshaper::AdditionalTracksType;

// categories that are not allowed to be shuffled with each other
#[derive(Debug, Clone, Copy)]
pub enum SongCategory {
    Looping,
    ShortNonLooping,
    NonLooping,
}

impl SongCategory {
    pub fn categorize(brstm: &BrstmInformation) -> Self {
        if brstm.info.loop_flag == 1 {
            Self::Looping
        } else if brstm.info.total_samples < 99300 {
            // 99300 samples is the arbitrary treshhold
            Self::ShortNonLooping
        } else {
            Self::NonLooping
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomMusicInfo {
    pub path: PathBuf,
    pub brstm_info: BrstmInformation,
    pub add_tracks_type: AdditionalTracksType,
}

pub struct MusicPack {
    // TODO: I want to move this around a lot, is a box worth it?
    pub songs: Vec<Box<CustomMusicInfo>>,
    pub replacements: HashMap<String, Box<CustomMusicInfo>>,
}

pub fn read_all_music_packs(dir: &Path) -> binrw::BinResult<Vec<MusicPack>> {
    let mut dirs = Vec::new();
    for result in fs::read_dir(dir)? {
        let entry = result?;
        if entry.metadata()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    dirs.iter().map(|dir| read_music_pack(dir)).collect()
}

// the file order is not deterministic!
pub fn read_music_pack(dir: &Path) -> binrw::BinResult<MusicPack> {
    // get all the song paths
    let mut songs = Vec::new();
    read_music_dir_rec(dir, 5, &mut songs)?;
    // read the replacement file if it exists
    let mut replacement_file_path = dir.to_owned();
    replacement_file_path.push("replacements.txt");
    let mut replacements = HashMap::new();
    if let Ok(f) = File::open(&replacement_file_path) {
        // since the file could be opened, *now* report errors during reading
        let reader = BufReader::new(f);
        for line in reader.lines() {
            let line = line?;
            if line
                .bytes()
                .next()
                .map_or(true, |b| !b.is_ascii_alphanumeric())
            {
                // ignore lines that we already know are not valid
                continue;
            }
            if let Some((vanilla, custom)) = line.split_once(':') {
                let vanilla = vanilla.trim();
                let custom = custom.trim();
                if vanilla.is_empty() || custom.is_empty() {
                    continue;
                }
                let custom_with_ext = OsString::from(format!("{custom}.brstm"));
                // find this custom song in the paths
                if let Some(pos) = songs
                    .iter()
                    .position(|s| s.path.file_name().unwrap() == custom_with_ext)
                {
                    replacements.insert(vanilla.to_string(), songs.swap_remove(pos));
                    // println!("success for {vanilla}, {custom}");
                } else {
                    // TODO: communicate a warning *somehow* better if the file is not found
                    eprintln!("replacement file {custom} can't be found!");
                }
            }
        }
    }
    Ok(MusicPack {
        songs,
        replacements,
    })
}

pub fn read_music_dir_rec(
    dir: &Path,
    max_depth: usize,
    songs: &mut Vec<Box<CustomMusicInfo>>,
) -> binrw::BinResult<()> {
    let new_depth = if let Some(new_depth) = max_depth.checked_sub(1) {
        new_depth
    } else {
        return Ok(());
    };
    for file in fs::read_dir(dir)? {
        match file {
            // TODO: print warning, collect errors?
            Err(_) => continue,
            Ok(entry) => {
                let path = entry.path();
                let path_meta = path.metadata()?;
                if path_meta.is_dir() {
                    read_music_dir_rec(&path, new_depth, songs)?;
                } else if path_meta.is_file() && path.extension().map_or(false, |e| e == "brstm") {
                    let read_file = || -> binrw::BinResult<_> {
                        let mut f = File::open(&path)?;
                        let result = BrstmInformation::from_reader(&mut f)?;
                        Ok(result)
                    };
                    match read_file() {
                        Ok(brstm) => {
                            songs.push(
                                CustomMusicInfo {
                                    path,
                                    add_tracks_type: AdditionalTracksType::categorize(&brstm),
                                    brstm_info: brstm,
                                }
                                .into(),
                            );
                        }
                        Err(e) => {
                            eprintln!("Error reading file: {e:?}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
