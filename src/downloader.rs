use curl::easy::Easy;
use std::path::Path;

pub struct Download {}

#[derive(Debug)]
pub enum Error {
    Curl(curl::Error),
    IO(std::io::Error),
}

impl From<curl::Error> for Error {
    fn from(error: curl::Error) -> Self {
        Error::Curl(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::IO(error)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for Error {}

impl Download {
    pub fn from_url<P>(url: &str, file_path: P) -> Result<(), Error>
    where
        P: AsRef<Path>,
    {
        let mut easy = Easy::new();
        easy.url(url)?;

        let mut buf = Vec::new();
        {
            let mut transfer = easy.transfer();
            transfer
                .write_function(|data| {
                    buf.extend_from_slice(data);
                    Ok(data.len())
                })
                .unwrap();
            transfer.perform()?;
        }
        std::fs::write(file_path, buf)?;
        Ok(())
    }
}
