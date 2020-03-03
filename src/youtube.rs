use anyhow::{Context, Result};
use log::{debug, trace};

use crate::common::{Service, YoutubeID};

static API_KEY: &str = "AIzaSyA8kgtG0_B8QWejoVD12B4OVoPwHS6Ax44"; // VIDL public API browser key (for Youtube API v3)

fn api_prefix() -> String {
    #[cfg(test)]
    let prefix: &str = &mockito::server_url();

    #[cfg(not(test))]
    let prefix: &str = "https://www.googleapis.com";

    prefix.into()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct YTChannelListResponse {
    kind: String,
    items: Vec<YTChannel>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTContentDetails {
    related_playlists: YTRelatedPlaylists,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTRelatedPlaylists {
    uploads: String,
    watch_history: String,
    watch_later: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTChannel {
    id: String,
    snippet: YTChannelSnippet,
    content_details: YTContentDetails,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct YTChannelSnippet {
    title: String,
    description: String,
    thumbnails: YTThumbnailList,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct YTThumbnailList {
    default: YTThumbnailInfo,
    medium: YTThumbnailInfo,
    high: YTThumbnailInfo,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct YTThumbnailInfo {
    url: String,
    width: u32,
    height: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTPlaylistItemListResponse {
    next_page_token: Option<String>,
    page_info: YTPlaylistPageInfo,
    items: Vec<YTPlaylistItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTPlaylistPageInfo {
    total_results: u64,
    results_per_page: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTPlaylistItemSnippetResource {
    kind: String,
    video_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTPlaylistItemSnippet {
    published_at: String,
    title: String,
    description: String,

    thumbnails: YTThumbnailList,

    channel_id: String,
    channel_title: String,
    playlist_id: String,

    resource_id: YTPlaylistItemSnippetResource,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct YTPlaylistItem {
    id: String,
    snippet: YTPlaylistItemSnippet,
}

/// Important info about channel
#[derive(Debug)]
pub struct ChannelMetadata {
    pub title: String,
    pub thumbnail: String,
    pub description: String,
}

/// Important info about a video
#[derive(Debug)]
pub struct VideoInfo {
    pub id: String,
    pub url: String,
    pub title: String,
    pub description: String,
    pub thumbnail_url: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

/// Helper to parse a `contentDetails` response from Youtube API
fn parse_content_details(url: &str) -> Result<YTChannelListResponse> {
    debug!("Retrieving URL {}", url);
    let resp = attohttpc::get(url).send()?;
    let text = resp.text()?;
    trace!("Raw response: {}", &text);
    let d: YTChannelListResponse = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse response from {}", &url))?;
    trace!("Raw deserialisation: {:?}", &d);
    Ok(d)
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
            "{prefix}/youtube/v3/channels?key={apikey}&id={chanid}&part=snippet%2CcontentDetails",
            prefix = api_prefix(),
            apikey = API_KEY,
            chanid = self.chan_id.id
        );
        let d = parse_content_details(&url)?;

        let chan = d.items.first().clone().context("Missing channel info")?;
        let cs = &chan.snippet;

        Ok(ChannelMetadata {
            title: cs.title.clone(),
            thumbnail: cs.thumbnails.default.url.clone(),
            description: cs.description.clone(),
        })
    }

    pub fn videos<'i>(&'i self) -> Result<impl Iterator<Item = Vec<VideoInfo>> + 'i> {
        // By ID:
        // https://www.googleapis.com/youtube/v3/channels?key={apikey}&id={chanid}&part=contentDetails
        // Or username:
        // https://www.googleapis.com/youtube/v3/channels?key={apikey}&forUsername={chanid}&part=contentDetails
        let url = format!(
            "{prefix}/youtube/v3/channels?key={apikey}&id={chanid}&part=snippet%2CcontentDetails",
            prefix = api_prefix(),
            apikey = API_KEY,
            chanid = self.chan_id.id
        );
        let d = parse_content_details(&url)?;

        let chan = d.items.first().clone().context("Missing channel info")?;
        let cd = chan.content_details.clone();
        let playlist_id = cd.related_playlists.uploads;

        let mut page_token: Option<String> = None;
        let mut complete = false;
        let it = std::iter::from_fn(move || {
            // If no more pages of video, stop
            if complete {
                return None;
            }

            // Get current page of videos
            let pl = self
                .get_playlist(&playlist_id, page_token.as_deref())
                .unwrap();

            // Store next page token for next iteration
            page_token = pl.next_page_token.clone();

            // If no next page token, store this info for next iteration
            if page_token.is_none() {
                complete = true;
            }

            // Videos to return
            let data: Vec<VideoInfo> = pl
                .items
                .iter()
                .map(|d| VideoInfo {
                    id: d.id.clone(),
                    url: format!(
                        "http://youtube.com/watch?v={id}",
                        id = d.snippet.resource_id.video_id
                    ),
                    title: d.snippet.title.clone(),
                    description: d.snippet.description.clone(),
                    thumbnail_url: d.snippet.thumbnails.default.url.clone(),
                    published_at: d
                        .snippet
                        .published_at
                        .parse::<chrono::DateTime<chrono::Utc>>()
                        .unwrap(),
                })
                .collect();
            Some(data)
        });
        Ok(it)
    }

    fn get_playlist(
        &self,
        playlist_id: &str,
        page_token: Option<&str>,
    ) -> Result<YTPlaylistItemListResponse> {
        // If no page token specified, start at beginning
        let pt = match page_token {
            None => "".into(),
            Some(val) => format!("&pageToken={}", val),
        };

        let url = format!("{prefix}/youtube/v3/playlistItems?key={apikey}&part=snippet&maxResults={num}&playlistId={playlist}{page}",
            prefix = api_prefix(),
            apikey=API_KEY,
            num=50,
            playlist=playlist_id,
            page=pt
        );
        debug!("Retrieving URL {:?}", &url);

        let resp = attohttpc::get(&url).send()?;
        let text = resp.text()?;
        let d: YTPlaylistItemListResponse = serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse response from URL {}", url))?;
        Ok(d)
    }
}

/// Find channel ID either from a username or ID
use crate::common::ChannelID;
pub fn find_channel_id(name: &str, service: &Service) -> Result<ChannelID> {
    match service {
        Service::Youtube => {
            debug!("Looking up by username");
            let url = format!(
                "{prefix}/youtube/v3/channels?key={apikey}&forUsername={name}&part=snippet%2CcontentDetails",
                prefix = api_prefix(),
                apikey=API_KEY,
                name=name);
            let d = parse_content_details(&url)?;

            if let Some(f) = d.items.first() {
                debug!("Found channel by username");
                Ok(ChannelID::Youtube(YoutubeID {
                    id: f.id.clone().into(),
                }))
            } else {
                debug!("Looking up by channel ID");
                let url = format!(
                    "{prefix}/youtube/v3/channels?key={apikey}&id={name}&part=snippet%2CcontentDetails",
                    prefix = api_prefix(),
                    apikey=API_KEY,
                    name=name);
                let d = parse_content_details(&url)?;

                if let Some(f) = d.items.first() {
                    debug!("Found channel by ID");
                    Ok(ChannelID::Youtube(YoutubeID {
                        id: f.id.clone().into(),
                    }))
                } else {
                    Err(anyhow::anyhow!(
                        "Could not find channel {} by username nor ID",
                        name
                    ))
                }
            }
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
        let vids = yt.videos()?;
        let result: Vec<super::VideoInfo> = vids.flatten().take(3).collect();
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
