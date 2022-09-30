pub enum ReshapeSrc {
    Track(u8),
    Empty,
}

#[derive(PartialEq, Eq)]
pub enum AdditionalTrackKind {
    Normal,
    Additive,
}

pub type AdditionalTracks = [AdditionalTrackKind];

pub fn calc_reshape(original: &AdditionalTracks, new: &AdditionalTracks) -> Vec<ReshapeSrc> {
    // main track always stays
    let mut result = Vec::with_capacity(new.len() + 1);
    result.push(ReshapeSrc::Track(0));
    let mut orig_normal_tracks = original
        .iter()
        .enumerate()
        .filter(|(_, typ)| **typ == AdditionalTrackKind::Normal)
        .map(|(i, _)| i as u8);
    let mut orig_additive_tracks = original
        .iter()
        .enumerate()
        .filter(|(_, typ)| **typ == AdditionalTrackKind::Additive)
        .map(|(i, _)| i as u8);
    // try to find a matching track in the original, otherwise use base track for normal and empty for additive
    for track in new.iter() {
        let reshape_entry = match track {
            AdditionalTrackKind::Normal => {
                ReshapeSrc::Track(orig_normal_tracks.next().unwrap_or(0))
            }
            AdditionalTrackKind::Additive => orig_additive_tracks
                .next()
                .map(|t| ReshapeSrc::Track(t))
                .unwrap_or(ReshapeSrc::Empty),
        };
        result.push(reshape_entry);
    }
    result
}
