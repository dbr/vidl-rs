use anyhow::Result;

/// Supported services
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Service {
    Youtube,
    Vimeo,
}

impl Service {
    pub fn as_str(&self) -> &str {
        match self {
            Service::Youtube => "youtube",
            Service::Vimeo => "vimeo",
        }
    }
    pub fn from_str(name: &str) -> Result<Self> {
        match name {
            "youtube" => Ok(Service::Youtube),
            "vimeo" => Ok(Service::Vimeo),
            _ => Err(anyhow::anyhow!("Unknown service string {:?}", name)),
        }
    }

    pub fn get_channel_id(&self, chanid_str: &str) -> ChannelID {
        match self {
            Service::Youtube => ChannelID::Youtube(YoutubeID {
                id: chanid_str.into(),
            }),
            Service::Vimeo => ChannelID::Vimeo(VimeoID {
                id: chanid_str.into(),
            }),
        }
    }
}

/// Identifier for channel on Youtube
#[derive(Debug, Clone, PartialEq)]
pub struct YoutubeID {
    pub id: String,
}

/// Identifier for channel on Vimeo
#[derive(Debug, Clone, PartialEq)]
pub struct VimeoID {
    pub id: String,
}

/// Identifier for a channel on a given service
#[derive(Debug, Clone, PartialEq)]
pub enum ChannelID {
    Youtube(YoutubeID),
    Vimeo(VimeoID),
}

impl ChannelID {
    pub fn id_str(&self) -> &str {
        match self {
            ChannelID::Vimeo(x) => &x.id,
            ChannelID::Youtube(x) => &x.id,
        }
    }
    pub fn service(&self) -> Service {
        match self {
            ChannelID::Vimeo(_) => Service::Vimeo,
            ChannelID::Youtube(_) => Service::Youtube,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum VideoStatus {
    /// New video
    New,

    /// Marked for download
    Queued,

    /// Being actively downloaded
    Downloading,

    /// Downloaded
    Grabbed,

    /// Error occured during download
    GrabError,

    /// Marked by user as uninteresting
    Ignore,
}

impl VideoStatus {
    pub fn as_str(&self) -> &str {
        match self {
            VideoStatus::New => "NE",
            VideoStatus::Queued => "QU",
            VideoStatus::Downloading => "DL",
            VideoStatus::Grabbed => "GR",
            VideoStatus::GrabError => "GE",
            VideoStatus::Ignore => "IG",
        }
    }

    pub fn from_str(status: &str) -> Result<Self> {
        match status {
            "NE" => Ok(VideoStatus::New),
            "QU" => Ok(VideoStatus::Queued),
            "DL" => Ok(VideoStatus::Downloading),
            "GR" => Ok(VideoStatus::Grabbed),
            "GE" => Ok(VideoStatus::GrabError),
            "IG" => Ok(VideoStatus::Ignore),
            _ => Err(anyhow::anyhow!("Unknown status string {:?}", status)),
        }
    }
}
