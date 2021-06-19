# vidl the Video Downloader

A web interface where you can add channels from Youtube etc. Videos from these channels are then cleanly listed, and can be downloaded with a single click for viewing without an internet connection.

## Architecture

Data is retrieved from Youtube via the [invidious][invidious] API.

Data is stored locally in an SQLite3 database. This includes a list of added channels, the videos within each channel, and their "status" (if queued for download, downloaded, etc)

Each video is represented as a `VideoInfo` object. This is generic enough to be applicable to every service. When retrieved from database, `VideoInfo` is wrapped in `DBVideoInfo` which adds some vidl or DB specific info (mainly ID and status - essentailly any info that wouldn't be known without the VIDL database)

[youtube-dl][youtube-dl] for offline caching of videos.

[invidious]: https://github.com/omarroth/invidious
[youtube-dl]: https://github.com/ytdl-org/youtube-dl/

## Installing

...

## Maintainance

Update youtube-dl:

    docker exec -it vidl pip install --upgrade youtube-dl
