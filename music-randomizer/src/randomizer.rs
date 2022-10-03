use std::{fs::File, io::BufWriter, path::PathBuf};

use binrw::io::BufReader;
use brstm::brstm::BrstmInformation;
use rand::{Rng, seq::SliceRandom};

use crate::{reshaper::{calc_reshape, reshape, AdditionalTracks, AdditionalTracksType}, vanilla_info::VanillaInfo};

fn do_shuffle<T>(v: &mut [T]) {}

#[derive(Debug)]
pub struct PatchEntry {
    pub vanilla_filename: &'static str,
    // TODO: if this is a vanilla song as well, there is no need to copy
    pub new_path: PathBuf,
    pub vanilla_type: AdditionalTracksType,
    pub new_type: AdditionalTracksType,
}

fn construct_path(base: &PathBuf, name: &str) -> PathBuf {
    let mut tmp = base.clone();
    tmp.push(name);
    tmp
}

// concrete datatypes TODO
pub fn randomize<R: Rng>(
    rng: &mut R,
    vanilla_songs: &Vec<VanillaInfo>,
    custom_songs: &Vec<(PathBuf, AdditionalTracksType)>,
    vanilla_path: &PathBuf,
) -> Vec<PatchEntry> {
    let mut custom_song_sample = custom_songs.clone();
    if custom_song_sample.len() < vanilla_songs.len() {
        // allow each song to appear max 2 times, to avoid vanilla music
        if custom_songs.len() * 2 < vanilla_songs.len() {
            custom_song_sample.extend(custom_songs.iter().cloned());
        } else {
            custom_song_sample.extend(custom_songs.choose_multiple(rng, vanilla_songs.len() - custom_songs.len()).cloned());
        }
    }
    // if this is still not enough, add vanilla music
    if custom_song_sample.len() < vanilla_songs.len() {
        custom_song_sample.extend(vanilla_songs.choose_multiple(rng, vanilla_songs.len() - custom_songs.len()).map(|vanilla_info| {
            (construct_path(vanilla_path, vanilla_info.name), vanilla_info.additional_tracks)
        }));
    }
    assert!(custom_song_sample.len() >= vanilla_songs.len());
    custom_song_sample.shuffle(rng);
    let patch_entries: Vec<_> = vanilla_songs.iter().zip(custom_song_sample.into_iter()).map(|(vanilla, custom)| {
        PatchEntry {
            new_path: custom.0,
            new_type: custom.1,
            vanilla_filename: vanilla.name,
            vanilla_type: vanilla.additional_tracks
        }
    }).collect();
    patch_entries
}

pub fn execute_patches(patches: &Vec<PatchEntry>, dest_folder: &PathBuf) -> binrw::BinResult<()> {
    for patch in patches.iter() {
        println!("patch {:?}", patch);
        let reshape_def = calc_reshape(
            patch.new_type.as_additional_tracks(),
            patch.vanilla_type.as_additional_tracks(),
        );
        let mut f = File::open(&patch.new_path)?;
        let mut new_song = BrstmInformation::from_reader(&mut f)?.into_with_data(&mut f)?;
        drop(f);
        new_song.fix_tracks();
        // TODO error handling
        println!("{reshape_def:?}");
        match reshape(&mut new_song, &reshape_def) {
            Ok(()) => {
                let mut f = File::create(construct_path(dest_folder, patch.vanilla_filename))?;
                new_song.write_brstm(&mut f)?;
            },
            Err(_e) => ()
        }
    }
    Ok(())
}
