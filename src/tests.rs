use crate::YoutubeQuery;

#[test]
fn test_channel_info() {
    let yt = YoutubeQuery::new("roosterteeth");
    let info = yt.info();
    let cd = info.contentDetails.unwrap();
}
