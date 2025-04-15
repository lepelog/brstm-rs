use std::path::Path;

use anyhow::{Context, bail};
use ffmpeg_next::ChannelLayout;

pub fn decode_channels<P: AsRef<Path>>(p: &P) -> anyhow::Result<(Vec<Vec<i16>>, u16)> {
    ffmpeg_next::init().expect("couldn't init ffmpeg");
    // println!("ffmpeg format version: {}", ffmpeg_next::format::version());
    // println!(
    //     "ffmpeg format config: {}",
    //     ffmpeg_next::format::configuration()
    // );
    // println!("ffmpeg codec version: {}", ffmpeg_next::codec::version());
    // println!(
    //     "ffmpeg codec config: {}",
    //     ffmpeg_next::codec::configuration()
    // );
    let mut opened = ffmpeg_next::format::input(p).with_context(|| format!("couldn't open {:?}", p.as_ref()))?;
    // println!("{:?}", opened.format().description());
    let audio_stream = opened
        .streams()
        .find(|stream| stream.parameters().medium() == ffmpeg_next::media::Type::Audio)
        .context("could not find audio stream")?;
    let audio_stream_index = audio_stream.index();
    let mut decoder = ffmpeg_next::codec::Context::from_parameters(audio_stream.parameters())
        .context("could not create decoder")?
        .decoder()
        .audio()
        .context("not an audio decoder")?;

    let mut encoder = ffmpeg_next::codec::Context::new().encoder().audio().unwrap();
    encoder.set_time_base(decoder.time_base());
    encoder.set_format(ffmpeg_next::format::Sample::I16(ffmpeg_next::format::sample::Type::Planar));
    if decoder.channel_layout().is_empty() {
        decoder.set_channel_layout(ChannelLayout::STEREO);
    }
    encoder.set_channel_layout(decoder.channel_layout());
    encoder.set_rate(decoder.rate() as _);
    let mut resampler = decoder.resampler(encoder.format(), encoder.channel_layout(), encoder.rate())
        .context("can't create resampler")?;
    let mut mp3_raw_frame = ffmpeg_next::util::frame::audio::Audio::empty();
    mp3_raw_frame.set_channel_layout(decoder.channel_layout());
    mp3_raw_frame.set_rate(decoder.rate());
    mp3_raw_frame.set_format(decoder.format());
    let mut i16_raw_frame = ffmpeg_next::util::frame::audio::Audio::empty();
    i16_raw_frame.set_channel_layout(encoder.channel_layout());
    i16_raw_frame.set_rate(encoder.rate());
    i16_raw_frame.set_format(encoder.format());
    let mut out_channels = vec![Vec::new(); decoder.channels() as usize];
    for (_, packet) in opened.packets() {
        if packet.stream() == audio_stream_index {
            decoder.send_packet(&packet).context("sending packet to decoder failed")?;
            receive_decoded_frames(
                &mut decoder,
                &mut resampler,
                &mut mp3_raw_frame,
                &mut i16_raw_frame,
                &mut out_channels,
            )?;
        }
    }
    decoder.flush();
    receive_decoded_frames(
        &mut decoder,
        &mut resampler,
        &mut mp3_raw_frame,
        &mut i16_raw_frame,
        &mut out_channels,
    )?;
    Ok((out_channels, decoder.rate() as u16))
}

// Ok(true) means continue, Ok(false) means break
fn map_receive_result(res: Result<(), ffmpeg_next::util::error::Error>) -> anyhow::Result<bool> {
    match res {
        Ok(()) => Ok(true),
        Err(e)
            if e == ffmpeg_next::util::error::Error::Eof
                || e == (ffmpeg_next::util::error::Error::Other {
                    errno: ffmpeg_next::util::error::EAGAIN,
                }) =>
        {
            Ok(false)
        }
        Err(e) => bail!("receive failed: {e:?}"),
    }
}

fn receive_decoded_frames(
    decoder: &mut ffmpeg_next::decoder::Audio,
    resampler: &mut ffmpeg_next::software::resampling::Context,
    mp3_raw_frame: &mut ffmpeg_next::frame::Audio,
    i16_raw_frame: &mut ffmpeg_next::frame::Audio,
    out: &mut [Vec<i16>],
) -> anyhow::Result<()> {
    while map_receive_result(decoder.receive_frame(mp3_raw_frame))? {
        // ???
        if mp3_raw_frame.channel_layout().is_empty() {
            mp3_raw_frame.set_channel_layout(ChannelLayout::STEREO);
        }
        let mut delay = resampler.run(mp3_raw_frame, i16_raw_frame).context("resampler failed")?;
        send_to_encode(i16_raw_frame, out)?;
        while delay.is_some() {
            delay = resampler.flush(i16_raw_frame)?;
            send_to_encode(i16_raw_frame, out)?;
        }
    }
    Ok(())
}

fn send_to_encode(i16_raw_frame: &mut ffmpeg_next::frame::Audio, out: &mut [Vec<i16>]) -> anyhow::Result<()> {
    assert_eq!(out.len(), i16_raw_frame.planes());
    for (plane_idx, chn) in out.iter_mut().enumerate() {
        chn.extend_from_slice(i16_raw_frame.plane(plane_idx));
    }
    Ok(())
}
