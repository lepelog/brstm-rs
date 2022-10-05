use std::{
    collections::{hash_map::Entry, HashMap},
    fs::File,
    path::{Path, PathBuf},
};

use brstm::brstm::BrstmInformation;
use rand::{seq::SliceRandom, Rng};

use crate::{
    loader::{CustomMusicInfo, MusicPack, SongCategory},
    reshaper::{calc_reshape, reshape, AdditionalTracksType},
    vanilla_info::VanillaInfo,
};
#[derive(Debug)]
pub struct PatchEntry {
    pub vanilla: VanillaInfo,
    pub custom: PatchTarget,
}

impl PatchTarget {
    pub fn get_add_track_type(&self) -> AdditionalTracksType {
        match self {
            PatchTarget::Custom(c) => c.add_tracks_type,
            PatchTarget::Vanilla(v) => v.add_tracks_type,
        }
    }
}

#[derive(Debug)]
pub enum PatchTarget {
    Custom(Box<CustomMusicInfo>),
    Vanilla(VanillaInfo),
}

fn construct_path(base: &Path, name: &str) -> PathBuf {
    let mut tmp = base.to_owned();
    tmp.push(name);
    tmp
}

pub fn randomize<R: Rng>(
    rng: &mut R,
    mut vanilla_songs: Vec<VanillaInfo>,
    music_packs: Vec<MusicPack>,
    shuffle_all: bool,
    limit_vanilla: bool,
) -> Vec<PatchEntry> {
    let mut patches = Vec::new();
    // pool of songs that can be freely randomized
    let mut randomized_pool = Vec::new();
    // the random setting ignores replacement requests
    if shuffle_all {
        for pack in music_packs.into_iter() {
            randomized_pool.extend(pack.songs);
            randomized_pool.extend(pack.replacements.into_values());
        }
    } else {
        // first, add the requested replacements
        let mut replacements: HashMap<String, Box<CustomMusicInfo>> = HashMap::new();
        // earlier packs have higher priority
        for pack in music_packs.into_iter() {
            randomized_pool.extend(pack.songs);
            for (vanilla_name, replacement) in pack.replacements {
                match replacements.entry(vanilla_name) {
                    Entry::Occupied(_) => {
                        // if it's already occupied, it will be randomized
                        randomized_pool.push(replacement);
                    }
                    Entry::Vacant(vac) => {
                        vac.insert(replacement);
                    }
                }
            }
        }
        // we can iterate on this map since we sort the randomized_pool afterwards
        for (vanilla_name, custom) in replacements {
            // find the file in the pack
            // hopefully this is fast enough
            if let Some(pos) = vanilla_songs.iter().position(|s| s.name == vanilla_name) {
                patches.push(PatchEntry {
                    vanilla: vanilla_songs.swap_remove(pos),
                    custom: PatchTarget::Custom(custom),
                })
            } else {
                // name is not found, just randomize it
                // TODO: better error handling
                eprintln!("vanilla song {vanilla_name} doesn't exist!");
                randomized_pool.push(custom);
            }
        }
    }
    randomized_pool.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    vanilla_songs.shuffle(rng);
    // place different types individually
    let mut vanilla_looping_songs = Vec::new();
    let mut vanilla_short_nonlooping_songs = Vec::new();
    let mut vanilla_long_nonlooping_songs = Vec::new();
    let mut custom_looping_songs = Vec::new();
    let mut custom_short_nonlooping_songs = Vec::new();
    let mut custom_long_nonlooping_songs = Vec::new();
    for vanilla in vanilla_songs {
        let list = match vanilla.category {
            SongCategory::Looping => &mut vanilla_looping_songs,
            SongCategory::ShortNonLooping => &mut vanilla_short_nonlooping_songs,
            SongCategory::NonLooping => &mut vanilla_long_nonlooping_songs,
        };
        list.push(vanilla);
    }
    for custom in randomized_pool {
        let list = match SongCategory::categorize(&custom.brstm_info) {
            SongCategory::Looping => &mut custom_looping_songs,
            SongCategory::ShortNonLooping => &mut custom_short_nonlooping_songs,
            SongCategory::NonLooping => &mut custom_long_nonlooping_songs,
        };
        list.push(custom);
    }
    let mut handle = |vanilla_songs: Vec<VanillaInfo>,
                      mut custom_songs: Vec<Box<CustomMusicInfo>>| {
        if limit_vanilla {
            let sample_count = vanilla_songs.len().saturating_sub(custom_songs.len());
            custom_songs.extend(
                custom_songs
                    .choose_multiple(rng, sample_count)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
        let vanilla_fill_necessary = vanilla_songs.len().saturating_sub(custom_songs.len());
        patches.extend(
            vanilla_songs
                .iter()
                .zip(
                    // try to use all custom songs
                    custom_songs.into_iter().map(PatchTarget::Custom).chain(
                        // but if that's not enough get vanilla songs, choosing randomly
                        vanilla_songs
                            .choose_multiple(rng, vanilla_fill_necessary)
                            .cloned()
                            .map(PatchTarget::Vanilla),
                    ),
                )
                .map(|(vanilla, custom)| PatchEntry {
                    vanilla: vanilla.clone(),
                    custom,
                }),
        );
    };
    handle(vanilla_looping_songs, custom_looping_songs);
    handle(
        vanilla_short_nonlooping_songs,
        custom_short_nonlooping_songs,
    );
    handle(vanilla_long_nonlooping_songs, custom_long_nonlooping_songs);
    patches
}

pub fn execute_patches(
    patches: Vec<PatchEntry>,
    vanilla_path: &Path,
    dest_folder: &Path,
) -> binrw::BinResult<()> {
    for patch in patches {
        println!("patching {}", patch.vanilla.name);
        // println!("patch {:?}", patch);
        let reshape_def = calc_reshape(
            patch.custom.get_add_track_type().as_additional_tracks(),
            patch.vanilla.add_tracks_type.as_additional_tracks(),
        );

        let mut new_song = match patch.custom {
            PatchTarget::Custom(c) => {
                let mut f = File::open(&c.path)?;
                c.brstm_info.into_with_data(&mut f)?
            }
            PatchTarget::Vanilla(v) => {
                let mut f = File::open(&construct_path(vanilla_path, v.name))?;
                BrstmInformation::from_reader(&mut f)?.into_with_data(&mut f)?
            }
        };
        new_song.fix_tracks();
        // TODO error handling
        // println!("{reshape_def:?}");
        match reshape(&mut new_song, &reshape_def) {
            Ok(()) => {
                let mut f = File::create(construct_path(dest_folder, patch.vanilla.name))?;
                new_song.write_brstm(&mut f)?;
            }
            Err(e) => eprintln!("Error: {e:?}"),
        }
    }
    Ok(())
}
