A note taking tool written in rust.

```
% lablog -h                                                                                  ~
lablog 0.1.0
lablog orders notes and todos into projects and subprojects without dictating a specific format

USAGE:
    lablog [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -D, --datadir <path>      the path to the lablog data dir [default: $XDG_DATA_HOME/lablog] 
    -l, --loglevel <level>
            the loglevel to run under [default: info]  [values: trace, debug, info, warn,
            error]

SUBCOMMANDS:
    dates       list dates when notes where taken
    help        Prints this message or the help of the given subcommand(s)
    migrate     migrate from the old csv format to the new one
    note        add a note to lablog
    notes       list all saved notes
    projects    list all projects available
    repo        interact with the notes repository
    web         start a webserver and serve notes over http
```
