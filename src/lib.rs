pub mod brstm;
pub mod structs;

#[derive(Debug)]
pub enum ReshapeSrc {
    Track(u8),
    Empty,
}

#[cfg(test)]
mod tests {}
