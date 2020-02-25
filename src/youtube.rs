use anyhow::{Context, Result};
use log::{debug, trace};

use crate::common::YoutubeID;

static API_KEY: &str = "AIzaSyBBUxzImakMKKW3B6Qu47lR9xMpb6DNqQE"; // ytdl public API browser key (for Youtube API v3)

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
struct YTPlaylistItemSnippet {
    published_at: String,
    title: String,
    description: String,

    thumbnails: YTThumbnailList,

    channel_id: String,
    channel_title: String,
    playlist_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct YTPlaylistItem {
    id: String,
    snippet: YTPlaylistItemSnippet,
}

/// Object to query data about given channel
#[derive(Debug)]
pub struct YoutubeQuery {
    chan_id: YoutubeID,
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
    pub title: String,
    pub description: String,
    pub thumbnail_url: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

impl<'a> YoutubeQuery {
    pub fn new(chan_id: YoutubeID) -> YoutubeQuery {
        YoutubeQuery { chan_id }
    }

    pub fn get_metadata(&self) -> Result<ChannelMetadata> {
        let url = format!(
            "https://www.googleapis.com/youtube/v3/channels?key={apikey}&forUsername={chanid}&part=snippet%2CcontentDetails",
            apikey=API_KEY,
            chanid=self.chan_id.id);
        debug!("Retrieving URL {}", &url);
        let resp = attohttpc::get(&url).send()?;
        let text = resp.text()?;
        trace!("Raw response: {}", &text);
        let d: YTChannelListResponse =
            serde_json::from_str(&text).context("Failed to parse response")?;
        trace!("Raw deserialisation: {:?}", &d);

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
            "https://www.googleapis.com/youtube/v3/channels?key={apikey}&forUsername={chanid}&part=snippet%2CcontentDetails",
            apikey=API_KEY,
            chanid=self.chan_id.id);
        debug!("Retrieving URL {}", &url);
        let resp = attohttpc::get(&url).send()?;
        let text = resp.text()?;
        trace!("Raw response: {}", &text);
        let d: YTChannelListResponse =
            serde_json::from_str(&text).context("Failed to parse response")?;
        trace!("Raw deserialisation: {:?}", &d);

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
                    title: d.snippet.title.clone(),
                    description: d.snippet.description.clone(),
                    thumbnail_url: d.snippet.thumbnails.default.url.clone(),
                    // published_at: time::strptime(&d.snippet.published_at, "Y-m-d\\TH:i:s.uP").unwrap_or(time::now())
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

        let url = format!("https://www.googleapis.com/youtube/v3/playlistItems?key={apikey}&part=snippet&maxResults={num}&playlistId={playlist}{page}",
            apikey=API_KEY,
            num=50,
            playlist=playlist_id,
            page=pt
        );
        debug!("Querying {:?}", &url);

        let resp = attohttpc::get(&url).send()?;
        let text = resp.text()?;
        let d: YTPlaylistItemListResponse =
            serde_json::from_str(&text).context("Faield to parse response")?;
        Ok(d)
    }
}
