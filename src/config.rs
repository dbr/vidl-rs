use directories::ProjectDirs;
use std::path::PathBuf;

pub struct Config {
    db_filepath: PathBuf,
    pub web_host: String,
    pub web_port: String,
    pub extra_youtubedl_args: Vec<String>,
    pub download_dir: PathBuf,
    pub filename_format: String,
    pub num_workers: usize,
}

impl Config {
    pub fn load() -> Config {
        let pd = ProjectDirs::from("uk.co", "dbrweb", "vidl")
            .expect("Unable to determine configuration directories");
        let cfg: PathBuf = PathBuf::from(pd.data_dir());

        let config_dir = std::env::var("VIDL_CONFIG_DIR")
            .and_then(|p| Ok(PathBuf::from(p)))
            .unwrap_or(cfg);
        let db_filepath = config_dir.join("vidl.sqlite3");

        Config {
            db_filepath,
            web_host: "0.0.0.0".into(),
            web_port: "8448".into(),
            extra_youtubedl_args: vec![
                "--restrict-filenames".into(),
                "--continue".into(),
                "-f".into(),
                "137/22/248/247/best".into(), // 1080p mp4, 720p mp4, 1080p webm, 720p webm, highest
            ],
            download_dir: PathBuf::from(
                std::env::var("VIDL_DOWNLOAD_DIR").unwrap_or("./download".into()),
            ),
            filename_format: "%(uploader)s__%(upload_date)s_%(title)s__%(id)s.%(ext)s".into(),
            num_workers: 4,
        }
    }

    pub fn db_filepath(&self) -> &PathBuf {
        &self.db_filepath
    }
}
