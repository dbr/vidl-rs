## Help

```console
$ vidl --help
Usage: vidl [OPTIONS] <COMMAND>

Commands:
  add       Add channel
  backup    Backup database as simple .json file
  download  enqueues videos for download
  init      Initialise the database
  list      list channels/videos
  migrate   update database schema to be current
  remove    remove given channel and all videos in it
  update    Updates all added channel info
  web       serve web interface
  worker    downloads queued videos
  help      Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help
  -V, --version     Print version

```

## Other commands

```console
$ vidl add --help
Add channel

Usage: vidl add [OPTIONS] <CHANID> [SERVICE]

Arguments:
  <CHANID>   
  [SERVICE]  youtube or vimeo [default: youtube] [possible values: youtube, vimeo]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl init --help
Initialise the database

Usage: vidl init [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl list --help
list channels/videos

Usage: vidl list [OPTIONS] [ID]

Arguments:
  [ID]  

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl update --help
Updates all added channel info

Usage: vidl update [OPTIONS] [FILTER]

Arguments:
  [FILTER]  Filter by channel name

Options:
  -f, --force        Checks for new data even if already updated recently
  -v, --verbose...   Verbosity level (can be specified multiple times)
      --full-update  Checks all pages, instead of stopping on an previously-seen video
  -h, --help         Print help

```

```console
$ vidl download --help
enqueues videos for download

Usage: vidl download [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl init --help
Initialise the database

Usage: vidl init [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl migrate --help
update database schema to be current

Usage: vidl migrate [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl remove --help
remove given channel and all videos in it

Usage: vidl remove [OPTIONS] <ID>

Arguments:
  <ID>  

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl web --help
serve web interface

Usage: vidl web [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl worker --help
downloads queued videos

Usage: vidl worker [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl backup --help
Backup database as simple .json file

Usage: vidl backup [OPTIONS] <COMMAND>

Commands:
  export  
  import  
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```

```console
$ vidl backup export --help
Usage: vidl backup export [OPTIONS]

Options:
  -o, --output <OUTPUT>  Output file
  -v, --verbose...       Verbosity level (can be specified multiple times)
  -h, --help             Print help

$ vidl backup import --help
Usage: vidl backup import [OPTIONS]

Options:
  -v, --verbose...  Verbosity level (can be specified multiple times)
  -h, --help        Print help

```