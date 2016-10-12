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
#![feature(custom_derive)]
#[macro_use]
extern crate clap;

#[macro_use]
extern crate log;

extern crate chrono;
extern crate csv;
extern crate env_logger;
extern crate loggerv;
extern crate rustc_serialize;
extern crate walkdir;
extern crate xdg;

use chrono::*;
use clap::App;
use clap::ArgMatches as Args;
use log::LogLevel;
use std::collections::BTreeMap as Map;
use std::collections::BTreeSet as Set;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use walkdir::WalkDir;
use xdg::BaseDirectories;

const PROJECT_SEPPERATOR: &'static str = ".";

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

    trace!("app: {:#?}", app);
    trace!("matches: {:#?}", matches);

    run(app)
}

fn run(args: Args) {
    match args.subcommand.clone() {
        Some(command) => {
            match command.name.as_str() {
                "note" => add_note(command.matches),
                "notes" => list_notes(command.matches),
                "projects" => list_projects(command.matches),
                "migrate" => migrate_notes(command.matches),
                _ => {
                    error!("do not know what to do with this command: {}",
                           command.name.as_str())
                }
            }
        }
        None => list_projects(args),
    }
}

fn migrate_notes(args: Args) {
    let datadir = get_datadir(&args);
    let sourcedir = PathBuf::from(args.value_of("sourcedir").unwrap());
    debug!("datadir: {:#?}", datadir);
    debug!("sourcedir: {:#?}", sourcedir);

    for file in WalkDir::new(&sourcedir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| match e.path().extension() {
            Some(ext) => ext.to_str().unwrap() == "csv",
            None => false,
        })
        .filter(|e| {
            !e.path().strip_prefix(&sourcedir).unwrap().to_str().unwrap().starts_with(".")
        }) {
        debug!("file: {:#?}", file.path());

        let mut rdr =
            csv::Reader::from_file(file.path()).unwrap().has_headers(false).flexible(true);

        for record in rdr.decode() {
            let (entry_type, time_stamp_raw, value): (String, String, String) = record.unwrap();
            trace!("entry_type: {}", entry_type);
            trace!("time_stamp_raw: {}", time_stamp_raw);
            trace!("value: {}", value);

            if entry_type != "note" {
                continue;
            }

            let project = file.path()
                .clone()
                .strip_prefix(&sourcedir).unwrap()
                .with_extension("")
                .to_str()
                .unwrap()
                .replace("/", PROJECT_SEPPERATOR);

            let time_stamp = time_stamp_raw.as_str().parse().unwrap();

            let note = Note {
                time_stamp: time_stamp,
                value: value,
            };
            trace!("datadir: {:#?}", datadir);
            trace!("project: {:#?}", project);

            write_note(&datadir, project.as_str(), note);
        }
    }
}

fn get_datadir(args: &Args) -> PathBuf {
    let datadir = match args.value_of("datadir").unwrap() {
        "$XDG_DATA_HOME/lablog" => {
            let xdg = BaseDirectories::new().unwrap();
            xdg.create_data_directory("lablog").unwrap()
        }
        _ => PathBuf::from(args.value_of("datadir").unwrap()),
    };
    debug!("datadir: {:?}", datadir);
    datadir
}

fn add_note(args: Args) {
    trace!("add_note args: {:#?}", args);
    let datadir = get_datadir(&args);
    let text = args.value_of("text").unwrap();
    let project = args.value_of("project").unwrap();
    debug!("project: {}", project);
    debug!("text: {}", text);

    let note = Note {
        time_stamp: UTC::now().into(),
        value: text.into(),
    };

    write_note(&datadir, project, note)
}

fn list_notes(args: Args) {
    trace!("list_notes args: {:#?}", args);
    let datadir = get_datadir(&args);
    let project_arg = args.value_of("project").unwrap();
    debug!("project_arg: {}", project_arg);

    let projects = match project_arg {
        "" => get_projects(&datadir),
        _ => {
            let mut out = Set::default();
            out.insert(project_arg.into());
            out
        }
    };

    let project_notes = get_projects_notes(&datadir, &projects);

    trace!("project_notes: {:#?}", project_notes);

    for (project, notes) in project_notes {
        println!("= {}", project);
        for (time_stamp, note) in notes {
            println!("== {}\n{}\n", time_stamp, note.value)
        }
    }
}

fn get_projects_notes<'a>(datadir: &PathBuf,
                          projects: &'a Set<String>)
                          -> Map<&'a str, Map<DateTime<UTC>, Note>> {
    let mut map = Map::default();

    for project in projects.iter().map(|x| x.as_str()) {
        let mut project_path = datadir.clone();
        project_path.push(normalize_project_path(project));

        let notes = get_notes(project_path);
        map.insert(project.clone(), notes);
    }

    map
}

fn list_projects(args: Args) {
    trace!("list_projects args: {:#?}", args);
    let datadir = get_datadir(&args);

    let projects = get_projects(&datadir);
    trace!("projects: {:#?}", projects);

    for project in projects {
        println!("{}", project)
    }
}

fn get_projects(datadir: &PathBuf) -> Set<String> {
    let ok_walkdir: Vec<walkdir::DirEntry> = WalkDir::new(&datadir)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    trace!("ok_walkdir: {:#?}", ok_walkdir);

    let stripped_paths: Vec<PathBuf> = ok_walkdir.iter()
        .filter(|e| e.path().is_file())
        .filter(|e| match e.path().extension() {
            Some(ext) => ext.to_str().unwrap() == "csv",
            None => false,
        })
        .map(|e| e.path().strip_prefix(&datadir))
        .filter_map(|e| e.ok())
        .filter(|e| !e.to_str().unwrap().starts_with("."))
        .map(|e| e.with_extension(""))
        .collect();

    trace!("stripped_paths: {:#?}", stripped_paths);

    let projects = stripped_paths.into_iter()
        .map(|e| e.to_str().unwrap().replace("/", PROJECT_SEPPERATOR))
        .collect();

    trace!("projects: {:#?}", projects);

    projects
}

fn get_notes(project_path: PathBuf) -> Map<DateTime<UTC>, Note> {
    let mut map = Map::default();
    let mut rdr = csv::Reader::from_file(project_path).unwrap().has_headers(false);

    for record in rdr.decode() {
        let note: Note = record.unwrap();

        trace!("note: {:#?}", note);

        map.insert(note.time_stamp, note);
    }

    map
}

fn normalize_project_path(project: &str) -> String {
    format!("{}.csv", project.replace(".", "/").as_str())
}

fn write_note(datadir: &PathBuf, project: &str, note: Note) {
    let mut project_path = datadir.clone();
    project_path.push(normalize_project_path(project));

    trace!("project_path: {:#?}", project_path);
    fs::create_dir_all(project_path.parent().unwrap()).unwrap();

    let mut file = match OpenOptions::new().append(true).open(&project_path) {
        Ok(file) => file,
        Err(_) => OpenOptions::new().append(true).create(true).open(&project_path).unwrap(),
    };

    let mut wtr = csv::Writer::from_memory();
    wtr.encode(note).unwrap();
    file.write_fmt(format_args!("{}", wtr.as_string())).unwrap();
}

#[derive(Debug,RustcEncodable,RustcDecodable)]
struct Note {
    time_stamp: DateTime<UTC>,
    value: String,
}
