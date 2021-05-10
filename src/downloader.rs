use curl::easy::Easy;
use std::path::Path;

pub struct Download {}

impl Download {
    pub fn from_url<P>(url: &str, file_path: P)
    where
        P: AsRef<Path>,
    {
        let mut easy = Easy::new();
        easy.url(url).unwrap();

        let mut buf = Vec::new();
        {
            let mut transfer = easy.transfer();
            transfer
                .write_function(|data| {
                    buf.extend_from_slice(data);
                    Ok(data.len())
                })
                .unwrap();
            transfer.perform().unwrap();
        }
        std::fs::write(file_path, buf).expect("couldn't store file to disk");
    }
}
