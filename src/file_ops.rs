use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use lazy_static::lazy_static;

lazy_static! {
    static ref PROJECT_DIRS: ProjectDirs = {
        let project_dirs = ProjectDirs::from("", "", "steam_server_disable").unwrap();

        // Create directories that are required
        log::info!("project data dir: {}", project_dirs.data_dir().to_str().unwrap());
        fs::create_dir_all(project_dirs.data_dir()).unwrap();

        project_dirs
    };
    static ref NETWORK_DATAGRAM_CONFIG_FILE_PATH: PathBuf = {
        let mut file_path = get_project_dirs().data_dir().to_path_buf();
        file_path.push("network_datagram_config.json");

        log::info!("network datagram config file: {}", file_path.to_str().unwrap());

        file_path
    };
}

pub fn get_project_dirs() -> &'static ProjectDirs {
    &PROJECT_DIRS
}

pub fn get_network_datagram_config_file_path() -> &'static Path {
    &NETWORK_DATAGRAM_CONFIG_FILE_PATH
}
