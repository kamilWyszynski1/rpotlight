pub mod cli;
pub mod db;
pub mod discoverer;
pub mod fts;
pub mod migrations;
pub mod model;
pub mod parse;
pub mod read;
pub mod registry;
pub mod twoway;
pub mod watcher;

pub mod communication {
    tonic::include_proto!("communication");
}

pub type GRPCResult<T> = Result<tonic::Response<T>, tonic::Status>;

use md5::Digest;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::Path;

pub fn file_checksum<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let f = File::open(path).unwrap();
    // Find the length of the file
    let len = f.metadata().unwrap().len();
    // Decide on a reasonable buffer size (1MB in this case, fastest will depend on hardware)
    let buf_len = len.min(1_000_000) as usize;
    let mut buf = BufReader::with_capacity(buf_len, f);
    let mut hasher = md5::Md5::default();

    io::copy(&mut buf, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use tempdir::TempDir;

    use super::file_checksum;

    #[test]
    fn test_file_checksum() -> anyhow::Result<()> {
        let tmp_dir = TempDir::new("example")?;
        let file_path = tmp_dir.path().join("test.txt");
        let mut file = File::create(&file_path)?;
        file.write_all("1qazxsw23edcvfr4".as_bytes())?;

        assert_eq!(
            // from https://www.md5.cz/
            "08b6e76863ebb1395c10eaa5f161a83f".to_string(),
            file_checksum(&file_path)?
        );

        Ok(())
    }
}
