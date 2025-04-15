use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::OsString,
    fs::{self, File},
    io::BufRead,
    path::{Path, PathBuf},
    rc::Rc,
};

use binrw::io::BufReader;
use brstm::{reshaper::AdditionalTrackKind, BrstmInformation};

use log::{debug, error, info};

use crate::NONLOOPNING_SHORT_CUTOFF_SECONDS;

//
// 1 stage
// 2 cs loop
// 3 cs no loop
// 4 2stream
// 5 2 stream add
// 6 3 stream
// 7 3 stream add
// 8 fanfare
// 9 effect
// 10 other
// 11 vanilla
// 12 2 add nonloop
// 13 3 (2std,3 add)

pub type AdditionalTracks = Cow<'static, [AdditionalTrackKind]>;

/// make a Cow with a list of additional tracks, for the count of additional normal tracks
pub fn make_normal_additional_tracks(add_count: usize) -> AdditionalTracks {
    use AdditionalTrackKind::*;
    match add_count {
        0 => Cow::Borrowed(&[]),
        1 => Cow::Borrowed(&[Normal]),
        2 => Cow::Borrowed(&[Normal, Normal]),
        3 => Cow::Borrowed(&[Normal, Normal, Normal]),
        4 => Cow::Borrowed(&[Normal, Normal, Normal, Normal]),
        5 => Cow::Borrowed(&[Normal, Normal, Normal, Normal, Normal]),
        // allocate a list for the more rare cases
        num => Cow::Owned(std::iter::repeat_n(Normal, num).collect()),
    }
}

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
        } else if brstm.info.total_samples
            < brstm.info.sample_rate as u32 * NONLOOPNING_SHORT_CUTOFF_SECONDS
        {
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
    pub add_tracks: Cow<'static, [AdditionalTrackKind]>,
}

pub struct MusicPack {
    pub songs: Vec<Rc<CustomMusicInfo>>,
    pub replacements: HashMap<String, Rc<CustomMusicInfo>>,
}

pub fn read_all_music_packs(dir: &Path) -> binrw::BinResult<Vec<MusicPack>> {
    let mut dirs = Vec::new();
    for result in fs::read_dir(dir)? {
        let entry = result?;
        if entry.metadata()?.is_dir() {
            let path = entry.path();
            // ignore hidden directories and directories starting with '_'
            // also ignore non UTF-8 cause idk how to deal with that otherwise
            if entry
                .file_name()
                .to_str()
                .is_some_and(|n| !n.starts_with('_') && !n.starts_with('.'))
            {
                info!("loading pack {:?}", &path);
                dirs.push(path);
            } else {
                info!("skipping {:?}", &path);
            }
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
    // holds songs that are removed from the list of normally randomized ones,
    // but they can appear multiple times if that's explicitly requested in replacements.txt
    let mut already_fixed_placed = Vec::new();
    if !replacement_file_path.is_file() {
        debug!("{replacement_file_path:?} is not a file (probably doesn't exist), skipping");
    } else if let Ok(f) = File::open(&replacement_file_path) {
        // since the file could be opened, *now* report errors during reading
        let reader = BufReader::new(f);
        for line in reader.lines() {
            let line = line?;
            if line
                .bytes()
                .next()
                .is_none_or(|b| !b.is_ascii_alphanumeric())
            {
                // ignore lines that we already know are not valid
                continue;
            }
            if let Some((vanilla, custom)) = line.split_once(':') {
                // TODO: support for also randomizing
                let vanilla = vanilla.trim();
                let custom = custom.trim();
                if vanilla.is_empty() || custom.is_empty() {
                    continue;
                }
                let custom_with_ext = OsString::from(format!("{custom}.brstm"));
                // find this custom song in the paths
                if let Some(pos) = songs
                    .iter()
                    .position(|s| s.path.file_name().is_some_and(|n| n == custom_with_ext))
                {
                    debug!("successfully found replacement for {vanilla}: {custom}");
                    let custom_replacement = songs.swap_remove(pos);
                    already_fixed_placed.push(Rc::clone(&custom_replacement));
                    replacements.insert(vanilla.to_string(), custom_replacement);
                } else if let Some(pos) = already_fixed_placed
                    .iter()
                    .position(|s| s.path.file_name().is_some_and(|n| n == custom_with_ext))
                {
                    debug!("successfully found replacement for {vanilla}: {custom} (again)");
                    replacements.insert(vanilla.to_string(), Rc::clone(&already_fixed_placed[pos]));
                } else {
                    // TODO: communicate a warning *somehow* better if the file is not found
                    error!("replacement file {custom} can't be found!");
                }
            }
        }
    } else {
        error!("could not open {replacement_file_path:?}, skipping");
    }
    Ok(MusicPack {
        songs,
        replacements,
    })
}

pub fn read_music_dir_rec(
    dir: &Path,
    max_depth: usize,
    songs: &mut Vec<Rc<CustomMusicInfo>>,
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
                } else if path_meta.is_file() && path.extension().is_some_and(|e| e == "brstm") {
                    let read_file = || -> binrw::BinResult<_> {
                        let f = File::open(&path)?;
                        let mut result = BrstmInformation::from_reader(&mut BufReader::new(f))?;
                        result.fix_tracks();
                        Ok(result)
                    };
                    match read_file() {
                        Ok(brstm) => {
                            debug!("successfully parsed {path:?}");
                            if let Some(additional_track_count) = brstm.tracks.len().checked_sub(1)
                            {
                                if brstm.channels_per_track().is_none() {
                                    error!(
                                        "File {path:?} has mixed mono/stereo information, skipping"
                                    );
                                } else {
                                    songs.push(Rc::new(CustomMusicInfo {
                                        path,
                                        // just
                                        add_tracks: make_normal_additional_tracks(
                                            additional_track_count,
                                        ),
                                        brstm_info: brstm,
                                    }));
                                }
                            } else {
                                error!("File {path:?} has 0 tracks, skipping");
                            }
                        }
                        Err(e) => {
                            error!("Error reading file {path:?}: {e:?}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
