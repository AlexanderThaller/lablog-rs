function __fish_using_command
    set cmd (commandline -opc)
    if [ (count $cmd) -eq (count $argv) ]
        for i in (seq (count $argv))
            if [ $cmd[$i] != $argv[$i] ]
                return 1
            end
        end
        return 0
    end
    return 1
end

complete -c lablog -n "__fish_using_command lablog" -s l -l loglevel -d "the loglevel to run under" -r -f -a "trace debug info warn error"
complete -c lablog -n "__fish_using_command lablog" -s D -l datadir -d "the path to the lablog data dir"
complete -c lablog -n "__fish_using_command lablog" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog" -f -a "note"
complete -c lablog -n "__fish_using_command lablog" -f -a "notes"
complete -c lablog -n "__fish_using_command lablog" -f -a "projects"
complete -c lablog -n "__fish_using_command lablog" -f -a "migrate"
complete -c lablog -n "__fish_using_command lablog" -f -a "web"
complete -c lablog -n "__fish_using_command lablog" -f -a "dates"
complete -c lablog -n "__fish_using_command lablog" -f -a "timeline"
complete -c lablog -n "__fish_using_command lablog" -f -a "repo"
complete -c lablog -n "__fish_using_command lablog" -f -a "help"
complete -c lablog -n "__fish_using_command lablog" -f -a "help"
complete -c lablog -n "__fish_using_command lablog note" -s f -l file -d "the file from which to read the note from"
complete -c lablog -n "__fish_using_command lablog note" -s e -l editor -d "open the default editor for writing the note"
complete -c lablog -n "__fish_using_command lablog note" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog note" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog note" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog note" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog notes" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog notes" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog notes" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog notes" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog projects" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog projects" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog projects" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog projects" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog migrate" -s S -l sourcedir -d "the folder from which to read the old files"
complete -c lablog -n "__fish_using_command lablog migrate" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog migrate" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog migrate" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog migrate" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog web" -s P -l port -d "the address and port to listen on"
complete -c lablog -n "__fish_using_command lablog web" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog web" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog web" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog web" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog dates" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog dates" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog dates" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog dates" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog timeline" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog timeline" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog timeline" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog timeline" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo" -f -a "pull"
complete -c lablog -n "__fish_using_command lablog repo" -f -a "push"
complete -c lablog -n "__fish_using_command lablog repo" -f -a "sync"
complete -c lablog -n "__fish_using_command lablog repo" -f -a "init"
complete -c lablog -n "__fish_using_command lablog repo" -f -a "help"
complete -c lablog -n "__fish_using_command lablog repo" -f -a "help"
complete -c lablog -n "__fish_using_command lablog repo pull" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo pull" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo pull" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo pull" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo push" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo push" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo push" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo push" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo sync" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo sync" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo sync" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo sync" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo init" -s R -l remote -d "clone from the remote instead of creating an empty repo"
complete -c lablog -n "__fish_using_command lablog repo init" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo init" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo init" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo init" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo help" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo help" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo help" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo help" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog repo help" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog repo help" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog help" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog help" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog help" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog help" -s V -l version -d "Prints version information"
complete -c lablog -n "__fish_using_command lablog help" -s h -l help -d "Prints help information"
complete -c lablog -n "__fish_using_command lablog help" -s V -l version -d "Prints version information"
