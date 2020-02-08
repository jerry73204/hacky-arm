use failure::Fallible;
use serde::Deserialize;
use std::{
    fs::File,
    io::{prelude::*, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub arm_device: PathBuf,
}

impl Config {
    pub fn open<P>(path: P) -> Fallible<Self>
    where
        P: AsRef<Path>,
    {
        let mut reader = BufReader::new(File::open(path)?);
        let mut string = String::new();
        reader.read_to_string(&mut string)?;
        let config: Self = json5::from_str(&string)?;
        Ok(config)
    }
}
