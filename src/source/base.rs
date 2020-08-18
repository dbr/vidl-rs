use anyhow::Result;

/// Important info about channel
#[derive(Debug)]
pub struct ChannelMetadata {
    pub title: String,
    pub thumbnail: String,
    pub description: String,
}

/// Important info about a video
pub struct VideoInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub description: String,
    pub thumbnail_url: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub duration: i32,
}

impl std::fmt::Debug for VideoInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VideoInfo{{id: {:?}, title: {:?}, url: {:?}, published_at: {:?}}}",
            self.id, self.title, self.url, self.published_at,
        )
    }
}

/// Source for info on a channel (collection of related videos - e.g a YouTube
/// channel, Vimeo user, etc), and access to videos within.
pub trait ChannelData {
    /// Get basic info on channel like title, icon URL etc
    fn get_metadata(&self) -> Result<ChannelMetadata>;

    /// Get an iterator over videos in channel, from newest to oldest. This
    /// should, ideally, lazily load videos from the source as the iterator will
    /// only be used until the most recently seen video
    fn videos<'i>(&'i self) -> Box<dyn Iterator<Item = Result<VideoInfo>> + 'i>;
}
