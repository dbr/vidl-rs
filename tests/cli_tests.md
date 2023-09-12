## Help

```console
$ vidl --help
vidl 

USAGE:
    vidl [FLAGS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

SUBCOMMANDS:
    add         Add channel
    backup      Backup database as simple .json file
    download    enqueues videos for download
    help        Prints this message or the help of the given subcommand(s)
    init        Initialise the database
    list        list channels/videos
    migrate     update database schema to be current
    remove      remove given channel and all videos in it
    update      Updates all added channel info
    web         serve web interface
    worker      downloads queued videos

```

# Setting up

Running a command like `list` will fail because no database exists yet

```console
$ vidl list
? failed
Error: unable to open database file: [..]
...
```

So initialize the database:

```console
$ vidl init
...
```

Running list now works, with no output:
```console
$ vidl list
```


## Other commands

```console
$ vidl add --help
vidl-add 
Add channel

USAGE:
    vidl add [FLAGS] <chanid> <youtube|vimeo>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

ARGS:
    <chanid>           
    <youtube|vimeo>     [default: youtube]  [possible values: youtube, vimeo]

$ vidl backup --help
vidl-backup 
Backup database as simple .json file

USAGE:
    vidl backup [FLAGS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

SUBCOMMANDS:
    export    export DB backup
    help      Prints this message or the help of the given subcommand(s)
    import    import DB backup

$ vidl download --help
vidl-download 
enqueues videos for download

USAGE:
    vidl download [FLAGS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

$ vidl init --help
vidl-init 
Initialise the database

USAGE:
    vidl init [FLAGS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

$ vidl list --help
vidl-list 
list channels/videos

USAGE:
    vidl list [FLAGS] [id]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

ARGS:
    <id>    

$ vidl migrate --help
vidl-migrate 
update database schema to be current

USAGE:
    vidl migrate [FLAGS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

$ vidl remove --help
vidl-remove 
remove given channel and all videos in it

USAGE:
    vidl remove [FLAGS] <id>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

ARGS:
    <id>    

$ vidl update --help
vidl-update 
Updates all added channel info

USAGE:
    vidl update [FLAGS] [filter]

FLAGS:
    -f                   Checks for new data even if already updated recently
        --full-update    Checks all pages, instead of stopping on an previously-seen video
    -h, --help           Prints help information
    -V, --version        Prints version information
    -v                   

ARGS:
    <filter>    

$ vidl web --help
vidl-web 
serve web interface

USAGE:
    vidl web [FLAGS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

$ vidl worker --help
vidl-worker 
downloads queued videos

USAGE:
    vidl worker [FLAGS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               

```