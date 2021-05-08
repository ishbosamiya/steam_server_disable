use curl::easy::Easy;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

pub struct Download {
    easy: Easy,
}

impl Download {
    pub fn from_url(url: &str) -> Self {
        let mut easy = Easy::new();
        easy.url(url).unwrap();

        Self { easy }
    }

    pub fn store_to_file<P>(&mut self, file_path: P)
    where
        P: AsRef<Path>,
    {
        let mut file = File::create(file_path).expect("could not create file");
        self.easy
            .write_function(move |data| {
                file.write_all(data).unwrap();
                Ok(data.len())
            })
            .unwrap();
        self.easy.perform().unwrap();
    }
}
