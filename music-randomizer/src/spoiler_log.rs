use std::{
    fs::{create_dir_all, File},
    io::{self, BufWriter, Write},
    path::Path,
};

use crate::randomizer::PatchEntry;

struct LogEntry<'a> {
    vanilla_name: &'a str,
    randomized_name: &'a str,
}

pub fn write_spoiler_log(path: &Path, seed: u64, entries: &[PatchEntry]) -> io::Result<()> {
    create_dir_all(path.parent().unwrap())?;
    let mut log_entries: Vec<_> = entries
        .iter()
        .map(|entry| LogEntry {
            vanilla_name: entry.vanilla.name,
            randomized_name: entry.custom.name().unwrap_or("INVALID"),
        })
        .collect();
    log_entries.sort_by_key(|e| e.vanilla_name);
    let mut log_file = BufWriter::new(File::create(path)?);
    writeln!(&mut log_file, "seed: {}", seed)?;
    for entry in &log_entries {
        writeln!(
            &mut log_file,
            "{}: {}",
            entry.vanilla_name, entry.randomized_name
        )?;
    }
    log_file.flush()?;
    Ok(())
}
