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

#[macro_use]
extern crate horrorshow;

extern crate chrono;
extern crate csv;
extern crate env_logger;
extern crate git2;
extern crate hyper;
extern crate iron;
extern crate loggerv;
extern crate router;
extern crate rustc_serialize;
extern crate walkdir;
extern crate xdg;

use chrono::*;
use clap::App;
use clap::ArgMatches as Args;
use git2::{Repository, Error};
use horrorshow::prelude::*;
use hyper::header::ContentType;
use hyper::mime::{Mime, TopLevel, SubLevel};
use iron::prelude::*;
use iron::status;
use log::LogLevel;
use router::Router;
use std::collections::BTreeMap as Map;
use std::collections::BTreeSet as DataSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::path::Path;
use std::process::{Command, Stdio};
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
                "web" => run_webapp(command.matches),
                "repo" => run_git(command.matches),
                _ => {
                    error!("do not know what to do with this command: {}",
                           command.name.as_str())
                }
            }
        }
        None => list_projects(args),
    }
}

fn run_git(args: Args) {
    match args.subcommand.clone() {
        Some(command) => {
            match command.name.as_str() {
                "init" => run_git_init(command.matches),
                _ => {
                    error!("do not know what to do with this command: {}",
                           command.name.as_str())
                }
            }
        }
        None => (),
    }
}

fn run_git_init(args: Args) {
    let datadir = get_datadir(&args);
    match args.value_of("remote") {
        Some(remote) => {
            Repository::clone(remote, datadir).unwrap();
            ()
        }
        None => {

            match Repository::open(&datadir) {
                Ok(_) => error!("repository is already initialized"),
                Err(_) => {
                    let repo = match Repository::init(&datadir) {
                        Ok(repo) => repo,
                        Err(e) => panic!("failed to init repository: {}", e),
                    };

                    let mut index = repo.index().unwrap();
                    index.add_all(vec!["*"].iter(), git2::ADD_DEFAULT, None).unwrap();
                    index.write().unwrap();
                    git_commit_init(&repo, "Initial commit").unwrap();
                }
            };
        }
    }
}

fn git_commit(repo: &Repository, msg: &str) -> Result<git2::Oid, git2::Error> {
    let signature = try!(repo.signature());

    let mut index = try!(repo.index());
    let tree_id = try!(index.write_tree());
    let tree = try!(repo.find_tree(tree_id));

    let head = try!(repo.head());
    let head_oid = head.target().unwrap();
    let head_commit = try!(repo.find_commit(head_oid));

    repo.commit(Some("HEAD"),
                &signature,
                &signature,
                &msg,
                &tree,
                &[&head_commit])
}

fn git_commit_init(repo: &Repository, message: &str) -> Result<(), Error> {
    // First use the config to initialize a commit signature for the user.
    let sig = try!(repo.signature());

    // Now let's create an empty tree for this commit
    let tree_id = {
        let mut index = try!(repo.index());

        // Outside of this example, you could call index.add_path()
        // here to put actual files into the index. For our purposes, we'll
        // leave it empty for now.

        try!(index.write_tree())
    };

    let tree = try!(repo.find_tree(tree_id));

    // Ready to create the initial commit.
    //
    // Normally creating a commit would involve looking up the current HEAD
    // commit and making that be the parent of the initial commit, but here this
    // is the first commit so there will be no parent.
    try!(repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[]));

    Ok(())
}

fn run_webapp(args: Args) {
    let listen_address = args.value_of("listen_address").unwrap();
    let datadir = get_datadir(&args);
    let datadir_clone = datadir.clone();
    let datadir_clone_clone = datadir.clone();

    let mut router = Router::new();
    router.get("/",
               move |r: &mut Request| webapp_projects(r, datadir.clone()),
               "projects");
    router.get("/notes/:project",
               move |r: &mut Request| webapp_notes(r, datadir_clone.clone()),
               "notes");
    router.get("/show/entries/:project",
               move |r: &mut Request| webapp_notes(r, datadir_clone_clone.clone()),
               "notes_legacy");

    info!("Listening on {}", listen_address);
    Iron::new(router).http(listen_address).unwrap();
}

fn webapp_notes(req: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let ref project = req.extensions.get::<Router>().unwrap().find("project").unwrap_or("_");
    debug!("project: {}", project);

    let out = format_or_cached(project, &datadir);

    let mut resp = Response::with((status::Ok, out));
    resp.headers.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])));
    Ok(resp)
}

fn format_or_cached(project: &str, datadir: &PathBuf) -> String {
    // TODO: Implement the cacheing
    let projects = projects_or_project(project, &datadir);
    let notes = get_projects_notes(&datadir, &projects);
    let notes_f = format_projects_notes(notes);

    let out = format_asciidoc(notes_f);

    out
}

fn format_asciidoc(input: String) -> String {
    let process = Command::new("asciidoctor")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("failed to execute process");

    process.stdin.unwrap().write_all(input.as_bytes()).unwrap();

    let mut out = String::new();
    process.stdout.unwrap().read_to_string(&mut out).unwrap();
    out
}

fn projects_or_project(project: &str, datadir: &PathBuf) -> DataSet<String> {
    match project {
        "_" => get_projects(datadir),
        _ => {
            let mut out = DataSet::default();
            out.insert(project.into());
            out
        }
    }
}

fn format_projects_notes<'a>(notes: Map<&'a str, Map<DateTime<UTC>, Note>>) -> String {
    let mut out = String::new();

    let header = include_str!("notes.header.asciidoc");

    out.push_str(header);

    for (project, notes) in notes {
        out.push_str(format!("== {}\n", project).as_str());
        for (time_stamp, note) in notes {
            out.push_str(format!("=== {}\n{}\n\n", time_stamp, note.value).as_str())
        }
    }

    out
}

fn webapp_projects(_: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let projects = get_projects(&datadir);

    let out = html!{
        html(lang="en") {
            head {
                title { : "Lablog - Projects" }
            }
            meta(charset="utf-8"){}
            body {
                h1 { : "Projects" }
                table {
                    @ for project in projects {
                        tr {
                            td {
                                a(href=format_args!("/notes/{}", project)) {
                                    : project
                                }
                            }
                        }
                    }
                }
            }
    }}
        .into_string()
        .unwrap();

    let mut resp = Response::with((status::Ok, out));
    resp.headers.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])));
    Ok(resp)
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
        trace!("file: {:#?}", file.path());

        let project = file.path()
            .clone()
            .strip_prefix(&sourcedir)
            .unwrap()
            .with_extension("")
            .to_str()
            .unwrap()
            .replace("/", PROJECT_SEPPERATOR);

        info!("Migrating project {}", project);

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

            let time_stamp = time_stamp_raw.as_str().parse().unwrap();

            let note = Note {
                time_stamp: time_stamp,
                value: value,
            };
            trace!("datadir: {:#?}", datadir);
            trace!("project: {:#?}", project);
            trace!("note: {:#?}", note);

            write_note(&datadir, project.as_str(), &note);
            git_commit_note(&datadir, project.as_str(), &note);
        }

        let mut rdr =
            csv::Reader::from_file(file.path()).unwrap().has_headers(false).flexible(true);
        for record in rdr.decode() {
            match record {
                Ok(_) => {}
                Err(_) => continue,
            };

            let (entry_type, time_stamp_raw, value1, value2): (String, String, String, String) =
                record.unwrap();

            trace!("entry_type: {}", entry_type);
            trace!("time_stamp_raw: {}", time_stamp_raw);
            trace!("value1: {}", value1);
            trace!("value2: {}", value2);

            if entry_type != "todo" {
                warn!("Don't know how to parse this entry type: {}", entry_type);
                continue;
            }

            let time_stamp = time_stamp_raw.as_str().parse().unwrap();

            let todo = Note {
                time_stamp: time_stamp,
                value: format!("TODO:: {}", value2),
            };
            trace!("datadir: {:#?}", datadir);
            trace!("project: {:#?}", project);
            trace!("todo: {:#?}", todo);

            write_note(&datadir, project.as_str(), &todo);
            git_commit_note(&datadir, project.as_str(), &todo);
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

    write_note(&datadir, project, &note);
    git_commit_note(&datadir, project, &note);
}

fn git_commit_note(datadir: &PathBuf, project: &str, note: &Note) {
    let project_path = normalize_project_path(project);

    let repo = Repository::open(&datadir).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(project_path.as_str())).unwrap();
    index.write().unwrap();

    let commit_message = format!("{} - {} - added", note.time_stamp, project);
    git_commit(&repo, commit_message.as_str()).unwrap();
}

fn list_notes(args: Args) {
    trace!("list_notes args: {:#?}", args);
    let datadir = get_datadir(&args);
    let project_arg = args.value_of("project").unwrap();
    debug!("project_arg: {}", project_arg);

    let projects = projects_or_project(project_arg, &datadir);
    let project_notes = get_projects_notes(&datadir, &projects);
    trace!("project_notes: {:#?}", project_notes);
    println!("{}", format_projects_notes(project_notes));
}

fn get_projects_notes<'a>(datadir: &PathBuf,
                          projects: &'a DataSet<String>)
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

fn get_projects(datadir: &PathBuf) -> DataSet<String> {
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

fn write_note(datadir: &PathBuf, project: &str, note: &Note) {
    if note.value == "" {
        warn!("Note with empty value");
        return;
    }

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
