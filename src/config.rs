use directories::ProjectDirs;
use std::path::PathBuf;

pub struct Config {
    db_filepath: PathBuf,
}

impl Config {
    pub fn load() -> Config {
        let pd = ProjectDirs::from("uk.co", "dbrweb", "ytdl")
            .expect("Unable to determine configuration directories");
        let cfg = pd.data_dir();
        let db_filepath = cfg.join("ytdl.sqlite3");
        Config {
            db_filepath: db_filepath,
        }
    }

    pub fn db_filepath(&self) -> &PathBuf {
        &self.db_filepath
    }
}
