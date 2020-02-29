use directories::ProjectDirs;
use std::path::PathBuf;

pub struct Config {
    db_filepath: PathBuf,
    pub web_host: String,
    pub web_port: String,
}

impl Config {
    pub fn load() -> Config {
        let pd = ProjectDirs::from("uk.co", "dbrweb", "vidl")
            .expect("Unable to determine configuration directories");
        let cfg = pd.data_dir();
        let db_filepath = cfg.join("vidl.sqlite3");
        Config {
            db_filepath: db_filepath,
            web_host: "0.0.0.0".into(),
            web_port: "8448".into(),
        }
    }

    pub fn db_filepath(&self) -> &PathBuf {
        &self.db_filepath
    }
}
