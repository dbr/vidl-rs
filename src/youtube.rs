use anyhow::{Context, Result};
use chrono::offset::TimeZone;

use log::{debug, trace};

use crate::common::{Service, YoutubeID};

fn api_prefix() -> String {
    #[cfg(test)]
    let prefix: &str = &mockito::server_url();

    #[cfg(not(test))]
    let prefix: &str = "https://invidio.us";

    prefix.into()
}

/*
[
  {
    title: String,
    videoId: String,
    author: String,
    authorId: String,
    authorUrl: String,

    videoThumbnails: [
      {
        quality: String,
        url: String,
        width: Int32,
        height: Int32
      }
    ],
    description: String,
    descriptionHtml: String,

    viewCount: Int64,
    published: Int64,
    publishedText: String,
    lengthSeconds: Int32
    paid: Bool,
    premium: Bool
  }
]
*/

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTVideoInfo {
    title: String,
    video_id: String,
    video_thumbnails: Vec<YTThumbnailInfo>,
    description: String,
    length_seconds: i32,
    paid: bool,
    premium: bool,
    published: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTThumbnailInfo {
    quality: Option<String>,
    url: String,
    width: i32,
    height: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTChannelInfo {
    author: String,
    author_id: String,
    description: String,
    author_thumbnails: Vec<YTThumbnailInfo>,
    author_banners: Vec<YTThumbnailInfo>,
}

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

fn request_data<T: serde::de::DeserializeOwned + std::fmt::Debug>(url: &str) -> Result<T> {
    fn subreq<T: serde::de::DeserializeOwned + std::fmt::Debug>(url: &str) -> Result<T> {
        debug!("Retrieving URL {}", &url);
        let resp = attohttpc::get(&url).send()?;
        let text = resp.text()?;
        trace!("Raw response: {}", &text);
        let data: T = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse response from {}", &url))?;
        trace!("Raw deserialisation: {:?}", &data);
        Ok(data)
    }
    let mut tries = 0;
    let ret: Result<T> = loop {
        let resp = subreq(url);
        if let Ok(data) = resp {
            break Ok(data);
        }
        debug!("Retrying request to {} because {:?}", &url, &resp);
        if tries > 3 {
            break resp;
        }
        tries += 1;
    };

    ret
}

/// Object to query data about given channel
#[derive(Debug)]
pub struct YoutubeQuery<'a> {
    chan_id: &'a YoutubeID,
}

impl<'a> YoutubeQuery<'a> {
    pub fn new(chan_id: &YoutubeID) -> YoutubeQuery {
        YoutubeQuery { chan_id }
    }

    pub fn get_metadata(&self) -> Result<ChannelMetadata> {
        let url = format!(
            "{prefix}/api/v1/channels/{chanid}",
            prefix = api_prefix(),
            chanid = self.chan_id.id
        );

        let d: YTChannelInfo = request_data(&url)?;

        Ok(ChannelMetadata {
            title: d.author.clone(),
            thumbnail: d.author_thumbnails[0].url.clone(),
            description: d.description.clone(),
        })
    }

    pub fn videos<'i>(&'i self) -> impl Iterator<Item = Result<VideoInfo>> + 'i {
        // GET /api/v1/channels/:ucid/videos?page=1

        fn get_page(chanid: &str, page: i32) -> Result<Vec<VideoInfo>> {
            let url = format!(
                "{prefix}/api/v1/channels/videos/{chanid}?page={page}",
                prefix = api_prefix(),
                chanid = chanid,
                page = page,
            );

            let data: Vec<YTVideoInfo> = request_data(&url)?;

            let ret: Vec<VideoInfo> = data
                .iter()
                .map(|d| VideoInfo {
                    id: d.video_id.clone(),
                    url: format!("http://youtube.com/watch?v={id}", id = d.video_id),
                    title: d.title.clone(),
                    description: d.description.clone(),
                    thumbnail_url: d.video_thumbnails.first().unwrap().url.clone(),
                    published_at: chrono::Utc.timestamp(d.published, 0),
                    duration: d.length_seconds,
                })
                .collect();

            Ok(ret)
        }

        let mut page_num = 1;
        use std::collections::VecDeque;
        let mut completed = false;
        let mut current_items: VecDeque<VideoInfo> = VecDeque::new();

        let it = std::iter::from_fn(move || -> Option<Result<VideoInfo>> {
            if completed {
                return None;
            }
            if let Some(cur) = current_items.pop_front() {
                // Iterate through previously stored items
                Some(Ok(cur))
            } else {
                // If nothing is stored, get next page of videos
                let data: Result<Vec<VideoInfo>> = get_page(&self.chan_id.id, page_num);
                page_num += 1; // Increment for future

                let nextup: Option<Result<VideoInfo>> = match data {
                    // Something went wrong, return an error item
                    Err(e) => {
                        // Error state, prevent future iteration
                        completed = true;
                        // Return error
                        Some(Err(e))
                    }
                    Ok(new_items) => {
                        if new_items.len() == 0 {
                            // No more items, stop iterator
                            None
                        } else {
                            current_items.extend(new_items);
                            Some(Ok(current_items.pop_front().unwrap()))
                        }
                    }
                };
                nextup
            }
        });
        it
    }
}

/// Find channel ID either from a username or ID
use crate::common::ChannelID;
pub fn find_channel_id(name: &str, service: &Service) -> Result<ChannelID> {
    match service {
        Service::Youtube => {
            debug!("Looking up by username");
            let url = format!(
                "{prefix}/api/v1/channels/{name}",
                prefix = api_prefix(),
                name = name
            );

            debug!("Retrieving URL {}", &url);
            let resp = attohttpc::get(&url).send()?;
            let text = resp.text().unwrap();
            trace!("Raw response: {}", &text);
            let data: YTChannelInfo = serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from {}", &url))?;
            trace!("Raw deserialisation: {:?}", &data);

            Ok(ChannelID::Youtube(YoutubeID { id: data.author_id }))
        }
        Service::Vimeo => Err(anyhow::anyhow!("Not yet implemented!")), // FIXME: This method belongs outside of youtube.rs
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_basic_find() -> Result<()> {
        let _m1 = mockito::mock("GET", "/api/v1/channels/thegreatsd")
            .with_body_from_file("testdata/channel_thegreatsd.json")
            .create();
        let _m2 = mockito::mock("GET", "/api/v1/channels/UCUBfKCp83QT19JCUekEdxOQ")
            .with_body_from_file("testdata/channel_thegreatsd.json") // Same content
            .create();

        let c = find_channel_id("thegreatsd", &crate::common::Service::Youtube)?;
        assert_eq!(c.id_str(), "UCUBfKCp83QT19JCUekEdxOQ");
        assert_eq!(c.service(), crate::common::Service::Youtube);

        // Check same `ChannelID` is found by ID as by username
        let by_id = find_channel_id("UCUBfKCp83QT19JCUekEdxOQ", &crate::common::Service::Youtube)?;
        assert_eq!(by_id, c);

        Ok(())
    }

    #[test]
    fn test_video_list() -> Result<()> {
        let mock_p1 = mockito::mock(
            "GET",
            "/api/v1/channels/videos/UCOYYX1Ucvx87A7CSy5M99yw?page=1",
        )
        .with_body_from_file("testdata/channel_climb_page1.json")
        .create();

        let mock_p2 = mockito::mock(
            "GET",
            "/api/v1/channels/videos/UCOYYX1Ucvx87A7CSy5M99yw?page=2",
        )
        .with_body_from_file("testdata/channel_climb_page2.json")
        .create();

        let cid = crate::common::YoutubeID {
            id: "UCOYYX1Ucvx87A7CSy5M99yw".into(),
        };
        let yt = YoutubeQuery::new(&cid);
        let vids = yt.videos();
        let result: Vec<super::VideoInfo> = vids
            .into_iter()
            .skip(58) // 60 videos per page, want to breach boundry
            .take(3)
            .collect::<Result<Vec<super::VideoInfo>>>()?;

        dbg!(&result);

        assert_eq!(result[0].title, "Vlog 013 - Excommunication");
        assert_eq!(result[1].title, "Vlog 012 - Only in America!");
        assert_eq!(
            result[2].title,
            "Vlog 011 - The part of the house no-one ever sees!"
        );

        assert_eq!(result[0].duration, 652);
        assert_eq!(result[1].duration, 562);
        assert_eq!(result[2].duration, 320);

        mock_p1.expect(1);
        mock_p2.expect(1);
        Ok(())
    }

    #[test]
    fn test_video_list_error() -> Result<()> {
        let mock_p1 = mockito::mock(
            "GET",
            "/api/v1/channels/videos/UCOYYX1Ucvx87A7CSy5M99yw?page=1",
        )
        .with_body("garbagenonsense")
        .create();

        let cid = crate::common::YoutubeID {
            id: "UCOYYX1Ucvx87A7CSy5M99yw".into(),
        };
        let yt = YoutubeQuery::new(&cid);
        let mut vids = yt.videos();
        assert!(vids.next().unwrap().is_err());
        mock_p1.expect(1);
        assert!(vids.next().is_none());
        Ok(())
    }

    #[test]
    fn test_metadata() -> Result<()> {
        let _m1 = mockito::mock("GET", "/api/v1/channels/UCUBfKCp83QT19JCUekEdxOQ")
            .with_body_from_file("testdata/channel_thegreatsd.json")
            .create();

        let cid = crate::common::YoutubeID {
            id: "UCUBfKCp83QT19JCUekEdxOQ".into(),
        };
        let yt = YoutubeQuery::new(&cid);
        let meta = yt.get_metadata()?;
        assert_eq!(meta.title, "thegreatsd");
        Ok(())
    }
}
