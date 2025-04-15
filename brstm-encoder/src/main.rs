use std::{fs::File, io::BufWriter};

use anyhow::{bail, Context};
use brstm::encoder::encode_brstm;
use clap::Parser;

mod ffmpeg;

#[derive(Parser)]
#[command(version)]
/// Encodes WAV files to BRSTM
pub struct Args {
    /// Path to the wav file to encode
    input_path: String,
    /// Path to the output brstm file, default <filename>.brstm
    brstm_path: Option<String>,
    #[arg(short = 'l', long)]
    /// If set, specifies the loop point
    r#loop: Option<u32>,
    #[arg(short = 'e', long)]
    /// If set, specifies the end point
    end: Option<u32>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let brstm_path = if let Some(path) = args.brstm_path {
        path
    } else {
        format!(
            "{}.brstm",
            args.input_path
                .rsplit_once(".")
                .context("The input file needs to have a file extension!")?.0
        )
    };
    let (mut channels, sampling_rate) = ffmpeg::decode_channels(&args.input_path).unwrap();
    if channels.is_empty() {
        bail!("no channels");
    }

    if let Some(end) = args.end {
        for channel in channels.iter_mut() {
            channel.truncate(end as usize);
        }
    }
    let sample_count = channels[0].len();
    if let Some(loop_point) = args.r#loop {
        println!(
            "encoding {} to {}, samples: {}, loop: {}",
            args.input_path, brstm_path, sample_count, loop_point
        );
    } else {
        println!(
            "encoding {} to {}, samples: {}, no loop",
            args.input_path, brstm_path, sample_count
        );
    }
    let out_brstm =
        encode_brstm(&channels, sampling_rate, args.r#loop).context("error encoding brstm")?;
    let mut out_file =
        BufWriter::new(File::create(&brstm_path).context("error creating out file")?);
    out_brstm
        .write_brstm(&mut out_file)
        .context("error writing out file")?;
    Ok(())
}
