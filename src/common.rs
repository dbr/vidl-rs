use anyhow::Result;

/// Supported services
#[derive(Debug, Clone, Copy)]
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
}

#[derive(Debug, Clone)]
pub struct YoutubeID {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct VimeoID {
    pub id: String,
}

#[derive(Debug, Clone)]
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
