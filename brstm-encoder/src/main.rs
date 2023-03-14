use std::{fs::File, io::BufWriter};

use anyhow::{bail, Context};
use brstm::encoder::encode_brstm;
use clap::Parser;
use wav::read_channels_from_wav;

mod wav;

#[derive(Parser)]
#[command(version)]
/// Encodes WAV files to BRSTM
pub struct Args {
    /// Path to the wav file to encode
    wav_path: String,
    /// Path to the output brstm file, default <filename>.brstm
    brstm_path: Option<String>,
    #[arg(short = 'l', long)]
    /// If set, specifies the loop point
    r#loop: Option<u32>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let brstm_path = if let Some(path) = args.brstm_path {
        path
    } else {
        format!(
            "{}.brstm",
            args.wav_path
                .strip_suffix(".wav")
                .context("The input file needs to have a 'wav' file extension!")?
        )
    };
    let wav_data = read_channels_from_wav(&args.wav_path).context("error reading wav file")?;
    if wav_data.channels.len() == 0 {
        bail!("no channels in wav");
    }
    let sample_count = wav_data.channels[0].len();
    if let Some(loop_point) = args.r#loop {
        println!(
            "encoding {} to {}, samples: {}, loop: {}",
            args.wav_path, brstm_path, sample_count, loop_point
        );
    } else {
        println!(
            "encoding {} to {}, samples: {}, no loop",
            args.wav_path, brstm_path, sample_count
        );
    }
    let out_brstm = encode_brstm(&wav_data.channels, wav_data.sample_rate, args.r#loop)
        .context("error encoding brstm")?;
    let mut out_file =
        BufWriter::new(File::create(&brstm_path).context("error creating out file")?);
    out_brstm
        .write_brstm(&mut out_file)
        .context("error writing out file")?;
    Ok(())
}
