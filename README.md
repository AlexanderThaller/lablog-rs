A note taking tool written in rust.

```
lablog 0.1.0
lablog orders notes into projects and subprojects without dictating a specific format

USAGE:
    lablog [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help
            Prints help information
    -V, --version
            Prints version information

OPTIONS:
    -D, --datadir <path>
            the path to the lablog data dir [default: $XDG_DATA_HOME/lablog]
    -l, --loglevel <level>
            the loglevel to run under [default: info]  [values: trace, debug, info, warn, error]

SUBCOMMANDS:
    dates
            list dates when notes where taken
    help
            Prints this message or the help of the given subcommand(s)
    note
            add a note to lablog
    notes
            list all saved notes
    projects
            list all projects available
    repo
            interact with the notes repository
    search
            search for given string in notes
    timeline
            list notes sorted by time
    timestamps
            list all timestamps for the project
```
