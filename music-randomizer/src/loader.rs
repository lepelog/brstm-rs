use std::{
    fs::{self, File},
    io, os,
    path::{Path, PathBuf},
};

use brstm::brstm::BrstmInformation;

use crate::reshaper::AdditionalTrackKind;

// categories that are not allowed to be shuffled with each other
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

pub struct SongCategorization {
    category: SongCategory,
    additional_tracks: Vec<AdditionalTrackKind>,
}

pub fn read_music_dir_rec(
    dir: &PathBuf,
    max_depth: usize,
    looping_songs: &mut Vec<(PathBuf, BrstmInformation)>,
    short_nonloops: &mut Vec<(PathBuf, BrstmInformation)>,
    long_nonloops: &mut Vec<(PathBuf, BrstmInformation)>,
) -> binrw::BinResult<()> {
    if max_depth == 0 {
        return Ok(());
    }
    for file in fs::read_dir(dir)? {
        match file {
            // TODO: print warning, collect errors?
            Err(_) => continue,
            Ok(entry) => {
                let path = entry.path();
                if path.is_dir() {
                    read_music_dir_rec(
                        &path,
                        max_depth - 1,
                        looping_songs,
                        short_nonloops,
                        long_nonloops,
                    )?;
                }
                let read_file = || -> binrw::BinResult<_> {
                    let mut f = File::open(&path)?;
                    let result = BrstmInformation::from_reader(&mut f)?;
                    Ok(result)
                };
                match read_file() {
                    Ok(brstm) => {
                        let list: &mut Vec<_> = match SongCategory::categorize(&brstm) {
                            SongCategory::Looping => looping_songs.as_mut(),
                            SongCategory::NonLooping => long_nonloops.as_mut(),
                            SongCategory::ShortNonLooping => short_nonloops.as_mut(),
                        };
                        list.push((path, brstm));
                    }
                    Err(e) => {
                        eprintln!("Error reading file: {e:?}");
                    }
                }
            }
        }
    }
    Ok(())
}
