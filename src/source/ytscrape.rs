use yt_chanvids::YtUploadsCrawler;

use anyhow::Result;

use crate::common::YoutubeID;
use crate::source::base::{ChannelData, ChannelMetadata, VideoInfo};

pub struct ScrapeQuery<'a> {
    chan_id: &'a YoutubeID,
}

impl<'a> ScrapeQuery<'a> {
    pub fn new(chan_id: &YoutubeID) -> ScrapeQuery {
        ScrapeQuery { chan_id }
    }
}

impl<'a> ChannelData for ScrapeQuery<'a> {
    fn get_metadata(&self) -> Result<ChannelMetadata> {
        let c = yt_chanvids::YtChannelDetailScraper::from_id(&self.chan_id.id).get();
        Ok(ChannelMetadata {
            title: c.title,
            thumbnail: c.author_thumbnail,
            description: "".into(),
        })
    }

    fn videos<'i>(&'i self) -> Box<dyn Iterator<Item = Result<VideoInfo>> + 'i> {
        let mut crawler = YtUploadsCrawler::channel(&self.chan_id.id);

        let it = std::iter::from_fn(move || -> Option<Result<VideoInfo>> {
            fn parse_date(input: &str) -> chrono::DateTime<chrono::Utc> {
                let nd = chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d")
                    .unwrap()
                    .and_hms(12, 0, 1);
                chrono::DateTime::from_utc(nd, chrono::Utc)
            }

            if let Some(link) = crawler.next() {
                let details = yt_chanvids::YtVideoDetailScraper::from_id(&link.id).get();
                let info = VideoInfo {
                    id: link.id.clone(),
                    url: format!("http://youtube.com/watch?v={}", &link.id),
                    title: link.title,
                    description: details.description,
                    thumbnail_url: link.thumbnail,
                    published_at: parse_date(&details.publish_date),
                    duration: details.duration_seconds,
                };
                Some(Ok(info))
            } else {
                None
            }
        });

        Box::new(it)
    }
}
