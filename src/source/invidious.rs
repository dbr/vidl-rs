use std::collections::VecDeque;

use anyhow::{Context, Result};
use chrono::offset::TimeZone;

use log::{debug, trace};

use crate::common::{Service, YoutubeID};
use crate::source::base::{ChannelMetadata, VideoInfo};

use ratelimit_meter::{DirectRateLimiter, GCRA};

fn api_prefix() -> String {
    #[cfg(test)]
    let prefix: String = mockito::server_url();

    #[cfg(not(test))]
    let prefix: String = std::env::var("VIDL_INVIDIOUS_URL")
        .ok()
        .unwrap_or_else(|| "https://y.com.sb".into());

    prefix
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
struct YtVideoPage {
    videos: Vec<YTVideoInfo>,
    continuation: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct YTVideoInfo {
    title: String,
    video_id: String,
    video_thumbnails: Vec<YTThumbnailInfo>,
    description: String,
    length_seconds: i32,
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

fn request_data<T: serde::de::DeserializeOwned + std::fmt::Debug>(url: &str) -> Result<T> {
    fn subreq<T: serde::de::DeserializeOwned + std::fmt::Debug>(url: &str) -> Result<T> {
        debug!("Retrieving URL {}", &url);
        let resp = attohttpc::get(&url)
        .header(
            attohttpc::header::USER_AGENT,
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:78.0) Gecko/20100101 Firefox/78.0",
        )
        .send()?;
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

/// Return the "default" quality thumbnail (falling back to the first)
fn choose_best_thumbnail(thumbs: &Vec<YTThumbnailInfo>) -> &YTThumbnailInfo {
    for t in thumbs {
        if t.quality == Some("default".into()) {
            return t;
        }
    }
    &thumbs[0]
}

/// Object to query data about given channel
#[derive(Debug)]
pub struct YoutubeQuery<'a> {
    chan_id: &'a YoutubeID,
    rate_limit: std::cell::RefCell<DirectRateLimiter<GCRA>>,
}

impl<'a> YoutubeQuery<'a> {
    pub fn new(chan_id: &YoutubeID) -> YoutubeQuery {
        YoutubeQuery {
            chan_id,
            rate_limit: std::cell::RefCell::new(DirectRateLimiter::<GCRA>::new(
                std::num::NonZeroU32::new(10).unwrap(),
                std::time::Duration::from_secs(60),
            )),
        }
    }
}

impl<'a> crate::source::base::ChannelData for YoutubeQuery<'a> {
    fn get_metadata(&self) -> Result<ChannelMetadata> {
        let url = format!(
            "{prefix}/api/v1/channels/{chanid}?fields=author,authorId,description,authorThumbnails,authorBanners",
            prefix = api_prefix(),
            chanid = self.chan_id.id
        );

        match self.rate_limit.borrow_mut().check() {
            Ok(_) => {
                // good
            }
            Err(_) => {
                trace!("Waiting for rate limit");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        let d: YTChannelInfo = request_data(&url)?;

        let thumbnail = choose_best_thumbnail(&d.author_thumbnails).url.clone();

        Ok(ChannelMetadata {
            title: d.author.clone(),
            thumbnail: thumbnail,
            description: d.description.clone(),
        })
    }

    fn videos<'i>(&'i self) -> Box<dyn Iterator<Item = Result<VideoInfo>> + 'i> {
        // GET /api/v1/channels/:ucid/videos?page=1

        enum Token {
            /// More pages to check
            Value(String),
            /// Nothing more
            End,
        }

        fn get_page(chanid: &str, continuation: &Option<Token>) -> Result<(Vec<VideoInfo>, Option<String>)> {
            let ct_arg = match continuation {
                Some(Token::Value(v)) => format!("?continuation={}", v),
                Some(Token::End) | None => "".into(),
            };

            let url = format!(
                "{prefix}/api/v1/channels/videos/{chanid}{continuation}",
                prefix = api_prefix(),
                chanid = chanid,
                continuation = ct_arg,
            );
            let data: YtVideoPage = request_data(&url)?;

            let ret: Vec<VideoInfo> = data
                .videos
                .iter()
                .map(|d| VideoInfo {
                    id: d.video_id.clone(),
                    url: format!("http://youtube.com/watch?v={id}", id = d.video_id),
                    title: d.title.clone(),
                    description: d.description.clone(),
                    thumbnail_url: choose_best_thumbnail(&d.video_thumbnails).url.clone(),
                    published_at: chrono::Utc.timestamp(d.published, 0),
                    duration: d.length_seconds,
                })
                .collect();

            Ok((ret, data.continuation))
        }

        let mut cont_token: Option<Token> = None;
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
                match self.rate_limit.borrow_mut().check() {
                    Ok(_) => {}
                    Err(_) => {
                        debug!("Waiting for rate limit");
                        std::thread::sleep(std::time::Duration::from_secs(10));
                    }
                }

                // If nothing is stored, get next page of videos
                let data: Result<(Vec<VideoInfo>, Option<String>)> = get_page(&self.chan_id.id, &cont_token);

                let nextup: Option<Result<VideoInfo>> = match data {
                    // Something went wrong, return an error item
                    Err(e) => {
                        // Error state, prevent future iteration
                        completed = true;
                        // Return error
                        Some(Err(e))
                    }
                    Ok((new_items, ct)) => {
                        match ct {
                            None => {
                                // No subsequent continuation, so no more requests needed
                                completed = true;
                            },
                            Some(ct) => {
                                cont_token = Some(Token::Value(ct));
                            },
                        }
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
        Box::new(it)
    }
}

pub(crate) mod workaround {
    struct Client {
        name: &'static str,
        version: &'static str,
        api_key: &'static str,
        screen: &'static str,
    }

    enum ClientType {
        Web,
        WebEmbeddedPlayer,
        WebMobile,
        WebScreenEmbed,
        Android,
        AndroidEmbeddedPlayer,
        AndroidScreenEmbed,
    }
    
    impl ClientType {
        fn details(&self) -> Client {
            match self {
                ClientType::Web => Client {
                    name:    "WEB",
                    version: "2.20210721.00.00",
                    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
                    screen:  "WATCH_FULL_SCREEN",
                  },
                  ClientType::WebEmbeddedPlayer => Client {
                    name:    "WEB_EMBEDDED_PLAYER", // 56
                    version: "1.20210721.1.0",
                    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
                    screen:  "EMBED",
                  },
                  ClientType::WebMobile => Client {
                    name:    "MWEB",
                    version: "2.20210726.08.00",
                    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
                    screen:  "", // None
                  },
                  ClientType::WebScreenEmbed => Client {
                    name:    "WEB",
                    version: "2.20210721.00.00",
                    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
                    screen:  "EMBED",
                  },
                  ClientType::Android => Client {
                    name:    "ANDROID",
                    version: "16.20",
                    api_key: "AIzaSyA8eiZmM1FaDVjRy-df2KTyQ_vz_yYM39w",
                    screen:  "", // ??
                  },
                  ClientType::AndroidEmbeddedPlayer => Client {
                    name:    "ANDROID_EMBEDDED_PLAYER", // 55
                    version: "16.20",
                    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
                    screen:  "", // None?
                  },
                  ClientType::AndroidScreenEmbed => Client {
                    name:    "ANDROID", // 3
                    version: "16.20",
                    api_key: "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8",
                    screen:  "EMBED",
                  },
                
            }
        }
    }
    
    pub struct Yt {
    }
    
    impl Yt {
        pub fn new() -> Self {
            Self {}
        }
    
        fn make_context(&self) -> serde_json::Value {
            let c = ClientType::WebScreenEmbed.details();
            serde_json::json!({
                "client": {
                    "hl"            : "en",
                    "gl"            : "US", // Can't be empty!
                    "clientName"    : c.name,
                    "clientVersion" : c.version,
                }
            })
        }
    
        /// Find channel ID (`UC..` string) based on either a user or channel name
        pub fn find_channel_id(&self, id: &str) -> anyhow::Result<String> {
            if id.starts_with("UC") {
                return Ok(id.into());
            }
    
            // Look up ID as a username
            let d = serde_json::json!({
                "context": self.make_context(),
                "url": format!("https://www.youtube.com/user/{}", id),
            });
            if let Ok(data) = self.post_json("/youtubei/v1/navigation/resolve_url".into(), d) {
                // Got response as user
                if let Some(browse_id) = data.pointer("/endpoint/browseEndpoint/browseId").and_then(|x| x.as_str()) {
                    Ok(browse_id.into())
                } else {
                    anyhow::bail!("Failed to find browseId for username");
                }
            } else {
                // Error as user, try as channel
                let d = serde_json::json!({
                    "context": self.make_context(),
                    "url": format!("https://www.youtube.com/c/{}", id),
                });
                if let Ok(data) = self.post_json("/youtubei/v1/navigation/resolve_url".into(), d) {
                    let d = data.pointer("/endpoint/browseEndpoint/browseId").and_then(|x| x.as_str()).unwrap();
                    Ok(d.into())
                } else {
                    anyhow::bail!("Failed to find browseId for {} (tried as channel and user)", &id);
                }
            }
        }
    
        fn post_json(&self, url: String, data: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            eprintln!("POST to {}", &url);
            eprintln!("data: {}", &data);
            let url = format!(
                "https://www.youtube.com/{url}?key={api_key}",
                url=url,
                api_key=ClientType::WebScreenEmbed.details().api_key);
            let req = attohttpc::post(&url)
            .header("Content-Type", "application/json; charset=UTF-8")
            .header("Accept-Encoding", "gzip")
            .text(serde_json::to_string(&data).unwrap())
            .send();
            let resp = req.unwrap();
            if resp.is_success() {
                let text = resp.text()?;
                let parsed: serde_json::Value = serde_json::from_str(&text)?;
                Ok(parsed)
            } else {
                anyhow::bail!("Error from {} - status {}", &url, resp.status())
            }
        }
    }
    
    #[test]
    fn test_basic() {
        let y = Yt::new();
    
        // Look up directly by channel ID
        assert_eq!(&y.find_channel_id("UCOYYX1Ucvx87A7CSy5M99yw").unwrap(), "UCOYYX1Ucvx87A7CSy5M99yw");
        // By channel name
        assert_eq!(&y.find_channel_id("onceuponaclimb").unwrap(), "UCOYYX1Ucvx87A7CSy5M99yw");
    
        // By username
        assert_eq!(&y.find_channel_id("thegreatsd").unwrap(), "UCUBfKCp83QT19JCUekEdxOQ");
    }
    
}

/// Find channel ID either from a username or ID
use crate::common::ChannelID;
pub fn find_channel_id(name: &str, service: &Service) -> Result<ChannelID> {
    match service {
        Service::Youtube => {
            let y = workaround::Yt::new();
            let id = y.find_channel_id(name)?;
            Ok(ChannelID::Youtube(YoutubeID { id }))
        }
        Service::Vimeo => Err(anyhow::anyhow!("Not yet implemented!")), // FIXME: This method belongs outside of youtube.rs
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::source::base::ChannelData;

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
