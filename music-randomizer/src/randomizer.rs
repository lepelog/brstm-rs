use log::{debug, error, info, log_enabled, Level};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    rc::Rc,
};

use brstm::{
    reshaper::{calc_reshape, reshape, AdditionalTrackKind},
    BrstmInformation,
};
use rand::{
    seq::{IndexedRandom, SliceRandom},
    Rng,
};

use crate::{
    loader::{CustomMusicInfo, MusicPack, SongCategory},
    vanilla_info::VanillaInfo,
};
#[derive(Debug)]
pub struct PatchEntry {
    pub vanilla: VanillaInfo,
    pub custom: PatchTarget,
}

impl PatchTarget {
    pub fn get_add_track_type(&self) -> &[AdditionalTrackKind] {
        match self {
            PatchTarget::Custom(c) => c.add_tracks.as_ref(),
            PatchTarget::Vanilla(v) => v.add_tracks,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            PatchTarget::Custom(c) => c.path.to_str(),
            PatchTarget::Vanilla(v) => Some(v.name),
        }
    }

    pub fn is_stereo(&self) -> bool {
        match self {
            PatchTarget::Custom(c) => c.brstm_info.is_stereo(),
            // all vanilla songs (we randomize) are stereo
            PatchTarget::Vanilla(..) => true,
        }
    }
}

#[derive(Debug)]
pub enum PatchTarget {
    Custom(Rc<CustomMusicInfo>),
    Vanilla(VanillaInfo),
}

fn construct_path(base: &Path, name: &str) -> PathBuf {
    let mut tmp = base.to_owned();
    tmp.push(name);
    tmp
}

trait VecRandChoiceRemove {
    type Item;
    fn choice_swap_remove<R: Rng>(&mut self, rng: &mut R) -> Option<Self::Item>;
}

impl<T> VecRandChoiceRemove for Vec<T> {
    type Item = T;

    fn choice_swap_remove<R: Rng>(&mut self, rng: &mut R) -> Option<Self::Item> {
        if self.is_empty() {
            None
        } else {
            Some(self.swap_remove(rng.random_range(0..self.len())))
        }
    }
}

pub fn only_set_fixed<R: Rng>(
    rng: &mut R,
    vanilla_songs: Vec<VanillaInfo>,
    music_packs: Vec<MusicPack>,
) -> Vec<PatchEntry> {
    let mut replacements: HashMap<String, Vec<Rc<CustomMusicInfo>>> = HashMap::new();
    for pack in music_packs.into_iter() {
        for (vanilla_name, replacement) in pack.replacements {
            replacements
                .entry(vanilla_name)
                .or_default()
                .push(replacement);
        }
    }
    vanilla_songs
        .into_iter()
        .map(|vanilla_song| match replacements.get(vanilla_song.name) {
            Some(custom_songs) => {
                let custom_song = custom_songs.choose(rng).unwrap();
                PatchEntry {
                    vanilla: vanilla_song,
                    custom: PatchTarget::Custom(Rc::clone(custom_song)),
                }
            }
            None => PatchEntry {
                vanilla: vanilla_song.clone(),
                custom: PatchTarget::Vanilla(vanilla_song),
            },
        })
        .collect()
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
        // if multiple packs replace the same song, choose one randomly and
        // just randomize the other ones
        let mut all_replacements: HashMap<String, Vec<Rc<CustomMusicInfo>>> = HashMap::new();
        for pack in music_packs.into_iter() {
            randomized_pool.extend(pack.songs);
            for (vanilla_name, replacement) in pack.replacements {
                all_replacements
                    .entry(vanilla_name)
                    .or_default()
                    .push(replacement);
            }
        }
        let mut pos = 0;
        while pos < vanilla_songs.len() {
            if let Some(mut replacement_songs) = all_replacements.remove(vanilla_songs[pos].name) {
                let chosen_song = replacement_songs.choice_swap_remove(rng).unwrap();
                for song in replacement_songs {
                    // randomize all other replacement songs
                    randomized_pool.push(song);
                }
                patches.push(PatchEntry {
                    vanilla: vanilla_songs.swap_remove(pos),
                    custom: PatchTarget::Custom(chosen_song),
                })
            } else {
                pos += 1;
            }
        }
        for leftover_replacement in all_replacements.keys() {
            error!("vanilla song {} doesn't exist", leftover_replacement);
        }
    }
    info!(
        "randomizing songs with {} in the pool",
        randomized_pool.len()
    );
    info!("{} songs already fixed placed", patches.len());
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
    info!("vanilla song counts by type:");
    info!("looping: {}", vanilla_looping_songs.len());
    info!("nonlooping short: {}", vanilla_short_nonlooping_songs.len());
    info!("nonlooping long: {}", vanilla_long_nonlooping_songs.len());
    for custom in randomized_pool {
        let list = match SongCategory::categorize(&custom.brstm_info) {
            SongCategory::Looping => &mut custom_looping_songs,
            SongCategory::ShortNonLooping => &mut custom_short_nonlooping_songs,
            SongCategory::NonLooping => &mut custom_long_nonlooping_songs,
        };
        list.push(custom);
    }
    info!("custom song counts by type:");
    info!("looping: {}", custom_looping_songs.len());
    info!("nonlooping short: {}", custom_short_nonlooping_songs.len());
    info!("nonlooping long: {}", custom_long_nonlooping_songs.len());
    let mut handle = |vanilla_songs: Vec<VanillaInfo>, custom_songs: &Vec<Rc<CustomMusicInfo>>| {
        // copy the pool, so we can reset it if we run out of tracks
        let mut copied_pool = custom_songs.clone();
        // vanilla songs to mix with the custom music
        let mut vanilla_shuffle_pool = vanilla_songs.clone();
        for vanilla_song in vanilla_songs {
            if custom_songs.is_empty() {
                // the vanilla pool is always big enough
                patches.push(PatchEntry {
                    vanilla: vanilla_song,
                    custom: PatchTarget::Vanilla(
                        vanilla_shuffle_pool.choice_swap_remove(rng).unwrap(),
                    ),
                });
            } else if let Some(custom_song) = copied_pool.choice_swap_remove(rng) {
                // found a song in the current pool
                patches.push(PatchEntry {
                    vanilla: vanilla_song,
                    custom: PatchTarget::Custom(custom_song),
                });
            } else {
                // if we get here, there is at least 1 custom song but the current custom pool is empty
                if limit_vanilla {
                    // never use vanilla songs, fill the pool again and choose from there
                    copied_pool = custom_songs.clone();
                    let custom_song = copied_pool.choice_swap_remove(rng).unwrap();
                    patches.push(PatchEntry {
                        vanilla: vanilla_song,
                        custom: PatchTarget::Custom(custom_song),
                    });
                } else {
                    // if the pool is exhausted and vanilla songs are allowed, use them
                    patches.push(PatchEntry {
                        vanilla: vanilla_song,
                        custom: PatchTarget::Vanilla(
                            vanilla_shuffle_pool.choice_swap_remove(rng).unwrap(),
                        ),
                    });
                }
            }
        }
    };
    handle(vanilla_looping_songs, &custom_looping_songs);
    handle(
        vanilla_short_nonlooping_songs,
        &custom_short_nonlooping_songs,
    );
    handle(vanilla_long_nonlooping_songs, &custom_long_nonlooping_songs);
    patches
}

pub fn execute_patches(
    patches: Vec<PatchEntry>,
    vanilla_path: &Path,
    dest_folder: &Path,
) -> binrw::BinResult<()> {
    for patch in patches {
        // TODO: this hopefully doesn't actually need a heap allocation
        let new_name = patch.custom.name().unwrap_or("<<INVALID>>").to_owned();
        if log_enabled!(Level::Debug) {
            debug!("replacing {} with {}", patch.vanilla.name, &new_name);
        } else {
            info!("patching {}", patch.vanilla.name);
        }
        // all tracks in vanilla (we randomize) are stereo
        let reshape_def = calc_reshape(
            patch.custom.get_add_track_type(),
            patch.custom.is_stereo(),
            patch.vanilla.add_tracks,
            true,
        );

        let mut new_song = match patch.custom {
            PatchTarget::Custom(c) => {
                let mut f = match File::open(&c.path) {
                    Err(e) => {
                        error!(
                            "Error opening custom file {} again, skipping: {e:?}",
                            &new_name
                        );
                        continue;
                    }
                    Ok(f) => f,
                };
                // TODO: use unwrap_or_clone once that gets stabilized
                let custom_info = Rc::try_unwrap(c).unwrap_or_else(|rc| (*rc).clone());
                match custom_info.brstm_info.into_with_data(&mut f) {
                    Err(e) => {
                        error!("Error reading song from {}, skipping: {e:?}", &new_name);
                        continue;
                    }
                    Ok(f) => f,
                }
            }
            PatchTarget::Vanilla(v) => {
                let mut f = File::open(&construct_path(vanilla_path, v.name))?;
                BrstmInformation::from_reader(&mut f)?.into_with_data(&mut f)?
            }
        };
        // TODO error handling
        // debug!("{reshape_def:?}");
        match reshape(&mut new_song, &reshape_def) {
            Ok(()) => {
                let outpath = construct_path(dest_folder, patch.vanilla.name);
                let mut f = match File::create(&outpath) {
                    Err(e) => {
                        error!("could not create outfile {:?}: {e:?}", outpath);
                        continue;
                    }
                    Ok(f) => f,
                };
                if let Err(e) = new_song.write_brstm(&mut f) {
                    error!(
                        "failed to write brstm {} to {}: {e:?}",
                        &new_name, patch.vanilla.name
                    );
                    continue;
                };
            }
            Err(e) => error!(
                "Error patching {} with {}: {e:?}",
                patch.vanilla.name, &new_name
            ),
        }
    }
    Ok(())
}
