#[derive(Debug, Clone)]
pub struct YoutubeID {
    pub id: String,
}

impl YoutubeID {
    pub fn service_str(&self) -> &str {
        "youtube"
    }
}

#[derive(Debug, Clone)]
pub struct VimeoID {
    pub id: String,
}

impl VimeoID {
    pub fn service_str(&self) -> &str {
        "vimeo"
    }
}

#[derive(Debug, Clone)]
pub enum ChannelID {
    Youtube(YoutubeID),
    Vimeo(VimeoID),
}
