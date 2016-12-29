// MIT License
//
// Copyright (c) 2016 Alexander Thaller <alexander@thaller.ws>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
#![feature(insert_str)]

extern crate chrono;
extern crate githelper;
extern crate lablog_lib;
extern crate loggerv;
extern crate regex;
extern crate tempdir;
extern crate xdg;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate log;

use chrono::*;
use clap::App;
use clap::ArgMatches as Args;
use lablog_lib::file_to_string;
use lablog_lib::filter_notes_by_timestamp;
use lablog_lib::format_projects_notes;
use lablog_lib::get_projects;
use lablog_lib::get_projects_notes;
use lablog_lib::get_timeline_for_notes;
use lablog_lib::git_commit_note;
use lablog_lib::Note;
use lablog_lib::ProjectsNotes;
use lablog_lib::try_multiple_time_parser;
use lablog_lib::write_note;
use log::LogLevel;
use regex::Regex;
use std::collections::BTreeMap as DataMap;
use std::collections::BTreeSet as DataSet;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempdir::TempDir;
use xdg::BaseDirectories;

#[derive(Debug)]
struct Options {
    datadir: PathBuf,
}

fn main() {
    let yaml = load_yaml!("cli.yml");
    let app = App::from_yaml(yaml)
        .version(crate_version!())
        .get_matches();

    let matches = match app.subcommand.clone() {
        Some(subcommand) => subcommand.matches,
        None => app.clone(),
    };

    let loglevel: LogLevel = app.value_of("loglevel").unwrap().parse().unwrap();
    loggerv::init_with_level(loglevel).unwrap();

    let options = get_options(&app);

    trace!("app: {:#?}", app);
    trace!("matches: {:#?}", matches);
    trace!("options: {:#?}", options);

    run(app, options)
}

fn run(args: Args, options: Options) {
    match args.subcommand.clone() {
        Some(command) => {
            match command.name.as_str() {
                "dates" => run_dates(command.matches, options),
                "note" => run_note(command.matches, options),
                "notes" => run_notes(command.matches, options),
                "timestamps" => run_timestamps(command.matches, options),
                "projects" => run_projects(command.matches, options),
                "repo" => run_repo(command.matches, options),
                "search" => run_search(command.matches, options),
                "timeline" => run_timeline(command.matches, options),
                _ => {
                    error!("do not know what to do with this command: {}",
                           command.name.as_str())
                }
            }
        }
        None => run_projects(args, options),
    }
}

fn run_dates(args: Args, options: Options) {
    let notes = get_filtered_notes(&args, &options);

    let mut dates = DataMap::default();
    for notes in notes.values() {
        for note in notes {
            *dates.entry(note.time_stamp.date()).or_insert(0) += 1;
        }
    }

    for (date, count) in dates {
        println!("{} {}", date, count)
    }
}

fn run_note(args: Args, options: Options) {
    trace!("run_note args: {:#?}", args);
    let project = args.value_of("project");

    let text = if args.is_present("editor") {
        string_from_editor()
    } else {
        match args.value_of("file") {
            Some(file_path) => {
                let file_text = file_to_string(Path::new(file_path)).unwrap();
                trace!("text from file: {}", file_text);
                file_text
            }
            None => {
                match args.value_of("text") {
                    Some(text) => String::from(text),
                    None => String::from(""),
                }
            }
        }
    };

    debug!("project: {:#?}", project);
    debug!("text: {}", text);

    let timestamp: DateTime<UTC> = match args.value_of("timestamp") {
        None => UTC::now().into(),
        Some(ts) => ts.parse().unwrap(),
    };

    let note = Note {
        time_stamp: timestamp,
        value: text.into(),
    };

    if write_note(&options.datadir, project, &note).is_some() {
        git_commit_note(&options.datadir, project, &note)
    }
}

fn run_notes(args: Args, options: Options) {
    let notes = get_filtered_notes(&args, &options);
    println!("{}", format_projects_notes(notes));
}

fn run_timestamps(args: Args, options: Options) {
    let notes = get_filtered_notes(&args, &options);

    for (project, notes) in notes {
        println!("{}", project);

        for (i, note) in notes.iter().enumerate() {
            println!("{}\t{}", i, note.time_stamp)
        }

        println!("");
    }
}

fn run_projects(args: Args, options: Options) {
    let projects = get_projects(&options.datadir, args.value_of("project"));
    trace!("projects: {:#?}", projects);

    for project in projects {
        println!("{}", project)
    }
}

fn run_repo(args: Args, options: Options) {
    if let Some(command) = args.subcommand.clone() {
        match command.name.as_str() {
            "init" => run_repo_init(&args, options),
            "pull" => run_repo_pull(options),
            "push" => run_repo_push(options),
            "sync" => run_repo_sync(options),
            _ => {
                error!("do not know what to do with this command: {}",
                       command.name.as_str())
            }
        }
    }
}

fn run_repo_pull(options: Options) {
    info!("pulling from remote repository");
    githelper::pull(options.datadir.as_path()).expect("can not pull from repo");
}

fn run_repo_push(options: Options) {
    info!("pushing to remote repository");
    githelper::push(options.datadir.as_path()).expect("can not push to repo");
}

fn run_repo_sync(options: Options) {
    info!("syncing with remote repository");
    githelper::sync(options.datadir.as_path()).expect("can not sync with repo");
}

fn run_repo_init(args: &Args, options: Options) {
    let datadir = options.datadir;

    match args.value_of("remote") {
        Some(remote) => {
            match githelper::clone(datadir.as_path(), remote) {
                Err(err) => error!("can not clone git repository from remote: {}", err),
                Ok(info) => info!("repo was cloned: {}", info),
            }
        }
        None => {
            match githelper::status(datadir.as_path()) {
                Ok(status) => error!("repository is already initialized: {}", status),
                Err(_) => {
                    githelper::init(datadir.as_path()).expect("can not initialize git repo");
                    githelper::commit(datadir.as_path(), "initial commit")
                        .expect("can not add initial commit to git repo");
                }
            };
        }
    }
}

fn run_search(args: Args, options: Options) {
    let text = args.value_of("text").unwrap();
    let notes = get_filtered_notes(&args, &options);

    let re = Regex::new(text).unwrap();

    let mut searched = DataMap::default();
    for (project, notes) in notes {
        for note in notes {
            for line in note.value.lines() {
                if re.is_match(line) {
                    debug!("match on line: {}", line);

                    let find = re.find(line).unwrap();

                    debug!("find: {:#?}", find);

                    let mut replaced = String::from(line);
                    replaced.insert_str(find.1, "\x1B[0m\x1B[0m");
                    replaced.insert_str(find.0, "\x1B[1m\x1B[31m");

                    debug!("replaced: {}", replaced);

                    searched.entry(project.clone())
                        .or_insert_with(DataSet::default)
                        .insert(replaced);
                }
            }
        }
    }

    let mut out = String::new();

    for (project, notes) in searched {
        out.push_str(format!("== {}\n", project).as_str());
        for note in notes {
            out.push_str(note.as_str());
            out.push_str("\n");
        }

        out.push_str("\n\n");
    }

    println!("{}", out);
}

fn run_timeline(args: Args, options: Options) {
    let notes = get_filtered_notes(&args, &options);
    println!("{}", get_timeline_for_notes(notes));
}

fn string_from_editor() -> String {
    let tmpdir = TempDir::new("lablog_tmp").unwrap();
    let tmppath = tmpdir.path().join("note.asciidoc");
    let editor = match env::var("VISUAL") {
        Ok(val) => val,
        Err(_) => {
            match env::var("EDITOR") {
                Ok(val) => val,
                Err(_) => panic!("Neither $VISUAL nor $EDITOR is set."),
            }
        }
    };

    let mut editor_command = Command::new(editor);
    editor_command.arg(tmppath.display().to_string());

    let editor_proc = editor_command.spawn();
    if editor_proc.expect("Couldn't launch editor").wait().is_ok() {
        file_to_string(&tmppath).unwrap()
    } else {
        panic!("The editor broke")
    }
}

fn get_options(args: &Args) -> Options {
    let datadir = match args.value_of("datadir").unwrap() {
        "$XDG_DATA_HOME/lablog" => {
            let xdg = BaseDirectories::new().unwrap();
            xdg.create_data_directory("lablog").unwrap()
        }
        _ => PathBuf::from(args.value_of("datadir").unwrap()),
    };

    let options = Options { datadir: datadir };

    debug!("options: {:?}", options);
    options
}

fn get_filtered_notes(args: &Args, options: &Options) -> ProjectsNotes {
    let projects = get_projects(&options.datadir, args.value_of("project"));
    let project_notes = get_projects_notes(&options.datadir, projects);

    let after_notes = match args.value_of("filter_after") {
        Some(filter) => {
            debug!("filter notes after {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from after parameter");
            debug!("filter notes after timestamp: {:#?}", timestamp);

            filter_notes_by_timestamp(project_notes, timestamp, true)
        }
        None => project_notes,
    };

    match args.value_of("filter_before") {
        Some(filter) => {
            debug!("filter notes before timestamp {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from parameter");
            debug!("filter notes before timestamp parsed: {:#?}", timestamp);

            filter_notes_by_timestamp(after_notes, timestamp, false)
        }
        None => after_notes,
    }
}
