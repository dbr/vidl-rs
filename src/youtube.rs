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
    height: i64,
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
}
impl std::fmt::Debug for VideoInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VideoInfo{{id: {:?}, title: {:?}, url: {:?}}}",
            self.id, self.title, self.url
        )
    }
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

        debug!("Retrieving URL {}", &url);
        let resp = attohttpc::get(&url).send()?;
        let text = resp.text()?;
        trace!("Raw response: {}", &text);
        let d: YTChannelInfo = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse response from {}", &url))?;
        trace!("Raw deserialisation: {:?}", &d);

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

            debug!("Retrieving URL {}", &url);
            let resp = attohttpc::get(&url).send()?;
            let text = resp.text().unwrap();
            trace!("Raw response: {}", &text);
            let data: Vec<YTVideoInfo> = serde_json::from_str(&text)
                .with_context(|| format!("Failed to parse response from {}", &url))?;
            trace!("Raw deserialisation: {:?}", &data);

            let ret: Vec<VideoInfo> = data
                .iter()
                .map(|d| VideoInfo {
                    id: d.video_id.clone(),
                    url: format!("http://youtube.com/watch?v={id}", id = d.video_id),
                    title: d.title.clone(),
                    description: d.description.clone(),
                    thumbnail_url: d.video_thumbnails.first().unwrap().url.clone(),
                    published_at: chrono::Utc.timestamp(d.published, 0),
                })
                .collect();

            Ok(ret)
        }

        let mut page_num = 1;
        let mut current_items: Vec<VideoInfo> = vec![];

        let it = std::iter::from_fn(move || -> Option<Result<VideoInfo>> {
            if let Some(cur) = current_items.pop() {
                // Iterate through previously stored items
                Some(Ok(cur))
            } else {
                // If nothing is stored, get next page of videos
                let data: Result<Vec<VideoInfo>> = get_page(&self.chan_id.id, page_num);
                page_num += 1; // Increment for future

                let nextup: Option<Result<VideoInfo>> = match data {
                    // Something went wrong, return an error item
                    Err(e) => Some(Err(e)),
                    Ok(new_items) => {
                        if new_items.len() == 0 {
                            // No more items, stop iterator
                            None
                        } else {
                            current_items.extend(new_items);
                            Some(Ok(current_items.pop().unwrap()))
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
        // FIXME: Mock HTTP responses
        let _m = mockito::mock(
            "GET",
            "/youtube/v3/channels?key=AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44&forUsername=roosterteeth&part=snippet%2CcontentDetails")
            .with_body_from_file("testdata/channel_rt.json")
           .create();
        let _m2 = mockito::mock(
            "GET",
            "/youtube/v3/channels?key=AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44&forUsername=UCzH3iADRIq1IJlIXjfNgTpA&part=snippet%2CcontentDetails"
            ).with_body_from_file("testdata/channel_rt_with_wrong_username.json")
            .create();
        let _m3 = mockito::mock(
            "GET",
            "/youtube/v3/channels?key=AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44&id=UCzH3iADRIq1IJlIXjfNgTpA&part=snippet%2CcontentDetails"
        )
            .with_body_from_file("testdata/channel_rt.json")
            .create();

        let c = find_channel_id("roosterteeth", &crate::common::Service::Youtube)?;
        assert_eq!(c.id_str(), "UCzH3iADRIq1IJlIXjfNgTpA");
        assert_eq!(c.service(), crate::common::Service::Youtube);

        // Check same `ChannelID` is found by ID as by username
        let by_id = find_channel_id("UCzH3iADRIq1IJlIXjfNgTpA", &crate::common::Service::Youtube)?;
        assert_eq!(by_id, c);

        Ok(())
    }

    #[test]
    fn test_video_list() -> Result<()> {
        let _m = mockito::mock("GET", "/youtube/v3/channels?key=AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44&id=UCzH3iADRIq1IJlIXjfNgTpA&part=snippet%2CcontentDetails")
            .with_body_from_file("testdata/channel_rt.json")
           .create();

        let _m2 = mockito::mock("GET", "/youtube/v3/playlistItems?key=AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44&part=snippet&maxResults=50&playlistId=UUzH3iADRIq1IJlIXjfNgTpA")
            .with_body_from_file("testdata/playlist_rt.json")
            .create();
        let cid = crate::common::YoutubeID {
            id: "UCzH3iADRIq1IJlIXjfNgTpA".into(),
        };
        let yt = YoutubeQuery::new(&cid);
        let vids = yt.videos();
        let result: Vec<super::VideoInfo> = vids
            .into_iter()
            .take(3)
            .collect::<Result<Vec<super::VideoInfo>>>()?;
        assert_eq!(result[0].title, "CAN WE LEARN TO DRIVE STICK? | RT Life");
        assert_eq!(
            result[1].title,
            "Pancake Podcast 2020 - Ep. #585  - RT Podcast"
        );
        assert_eq!(
            result[2].title,
            "Sleepy Plane Stories - Rooster Teeth Animated Adventures"
        );
        dbg!(result);
        Ok(())
    }

    #[test]
    fn test_metadata() -> Result<()> {
        let _m = mockito::mock("GET", "/youtube/v3/channels?key=AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44&id=UCzH3iADRIq1IJlIXjfNgTpA&part=snippet%2CcontentDetails")
            .with_body_from_file("testdata/channel_rt.json")
            .create();

        let cid = crate::common::YoutubeID {
            id: "UCzH3iADRIq1IJlIXjfNgTpA".into(),
        };
        let yt = YoutubeQuery::new(&cid);
        let meta = yt.get_metadata()?;
        assert_eq!(meta.title, "Rooster Teeth");
        Ok(())
    }
}
