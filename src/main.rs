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
#![feature(insert_str)]

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
extern crate githelper;
extern crate hyper;
extern crate iron;
extern crate libc;
extern crate logger;
extern crate loggerv;
extern crate regex;
extern crate router;
extern crate rustc_serialize;
extern crate tempdir;
extern crate url;
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
use logger::Logger;
use log::LogLevel;
use regex::Regex;
use router::Router;
use rustc_serialize::json;
use std::collections::BTreeMap as DataMap;
use std::collections::BTreeSet as DataSet;
use std::env;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Result as IOResult;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempdir::TempDir;
use url::percent_encoding;
use walkdir::WalkDir;
use xdg::BaseDirectories;

const PROJECT_SEPPERATOR: &'static str = ".";
const ALL_PROJECTS: &'static str = "_";

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
                "web" => run_webapp(command.matches),
                "repo" => run_git(command.matches),
                "dates" => run_dates(command.matches),
                "timeline" => run_timeline(command.matches),
                "search" => run_search(command.matches),
                _ => {
                    error!("do not know what to do with this command: {}",
                           command.name.as_str())
                }
            }
        }
        None => list_projects(args),
    }
}

fn run_search(args: Args) {
    trace!("run_search args: {:#?}", args);
    let datadir = get_datadir(&args);
    let project_arg = args.value_of("project").unwrap();
    let text = args.value_of("text").unwrap();
    debug!("project_arg: {}", project_arg);
    debug!("text: {}", text);

    let projects = projects_or_project(project_arg, &datadir);
    let project_notes = get_projects_notes(&datadir, &projects);
    let before_notes = match args.value_of("filter_before") {
        Some(filter) => {
            debug!("filter notes before timestamp {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from parameter");
            debug!("filter notes before timestamp parsed: {:#?}", timestamp);

            filter_notes_by_timestamp(project_notes, timestamp, true)
        }
        None => project_notes,
    };

    let after_notes = match args.value_of("filter_after") {
        Some(filter) => {
            debug!("filter notes after {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from after parameter");
            debug!("filter notes before timestamp: {:#?}", timestamp);

            filter_notes_by_timestamp(before_notes, timestamp, false)
        }
        None => before_notes,
    };
    trace!("project_notes: {:#?}", after_notes);
    let re = Regex::new(text).unwrap();

    let mut searched = DataMap::default();
    for (project, notes) in after_notes {
        for note in notes.values() {
            for line in note.value.lines() {
                if re.is_match(line) {
                    debug!("match on line: {}", line);

                    let find = re.find(line).unwrap();

                    debug!("find: {:#?}", find);

                    let mut replaced = String::from(line);
                    replaced.insert_str(find.1, "\x1B[0m\x1B[0m");
                    replaced.insert_str(find.0, "\x1B[1m\x1B[31m");

                    debug!("replaced: {}", replaced);

                    searched.entry(project)
                        .or_insert_with(DataSet::default)
                        .insert(replaced);
                }
            }
        }
    }

    let mut out = String::new();

    let header = include_str!("notes.header.asciidoc");
    out.push_str(header);

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

fn run_timeline(args: Args) {
    let datadir = get_datadir(&args);
    let project = args.value_of("project").unwrap();
    let projects = projects_or_project(project, &datadir);
    let project_notes = get_projects_notes(&datadir, &projects);

    let before_notes = match args.value_of("filter_before") {
        Some(filter) => {
            debug!("filter notes before timestamp {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from parameter");
            debug!("filter notes before timestamp parsed: {:#?}", timestamp);

            filter_notes_by_timestamp(project_notes, timestamp, true)
        }
        None => project_notes,
    };

    let after_notes = match args.value_of("filter_after") {
        Some(filter) => {
            debug!("filter notes after {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from after parameter");
            debug!("filter notes before timestamp: {:#?}", timestamp);

            filter_notes_by_timestamp(before_notes, timestamp, false)
        }
        None => before_notes,
    };

    println!("{}", get_timeline_for_notes(after_notes));
}

fn get_timeline_for_notes(notes: DataMap<&str, DataMap<DateTime<UTC>, Note>>) -> String {
    let mut timeline = DataMap::default();
    for (project, notes) in notes {
        for (timestamp, note) in notes {
            timeline.entry(timestamp.date())
                .or_insert_with(DataMap::default)
                .entry(project)
                .or_insert_with(DataMap::default)
                .insert(timestamp, note);
        }
    }

    let mut out = String::new();
    let header = include_str!("timeline.header.asciidoc");
    out.push_str(header);

    let indentreg = Regex::new(r"(?m)^=").unwrap();
    let indentrepl = "=====";
    for (timestamp, projects) in timeline {
        out.push_str(format!("== {}\n", timestamp).as_str());
        for (project, notes) in projects {
            out.push_str(format!("=== {}\n", project).as_str());
            for (timestamp, note) in notes {
                let indentnote = indentreg.replace_all(note.value.as_str(), indentrepl);
                out.push_str(format!("==== {}\n{}\n\n", timestamp, indentnote).as_str());
            }
        }
    }

    out
}

fn get_timeline(project: &str, datadir: &PathBuf) -> String {
    let projects = projects_or_project(project, datadir);
    let project_notes = get_projects_notes(datadir, &projects);

    get_timeline_for_notes(project_notes)
}

fn run_dates(args: Args) {
    let datadir = get_datadir(&args);
    let project_arg = args.value_of("project").unwrap();
    let projects = projects_or_project(project_arg, &datadir);
    let project_notes = get_projects_notes(&datadir, &projects);

    let before_notes = match args.value_of("filter_before") {
        Some(filter) => {
            debug!("filter notes before timestamp {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from parameter");
            debug!("filter notes before timestamp parsed: {:#?}", timestamp);

            filter_notes_by_timestamp(project_notes, timestamp, true)
        }
        None => project_notes,
    };

    let after_notes = match args.value_of("filter_after") {
        Some(filter) => {
            debug!("filter notes after {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from after parameter");
            debug!("filter notes before timestamp: {:#?}", timestamp);

            filter_notes_by_timestamp(before_notes, timestamp, false)
        }
        None => before_notes,
    };

    let mut dates = DataMap::default();
    for notes in after_notes.values() {
        for timestamp in notes.keys() {
            *dates.entry(timestamp.date()).or_insert(0) += 1;
        }
    }

    for (date, count) in dates {
        println!("{} {}", date, count)
    }
}

fn run_git(args: Args) {
    if let Some(command) = args.subcommand.clone() {
        match command.name.as_str() {
            "init" => run_git_init(&command.matches),
            "pull" => run_git_pull(&command.matches),
            "push" => run_git_push(&command.matches),
            "sync" => run_git_sync(&command.matches),
            _ => {
                error!("do not know what to do with this command: {}",
                       command.name.as_str())
            }
        }
    }
}

fn run_git_pull(args: &Args) {
    let datadir = get_datadir(args);
    let output = Command::new("git")
        .arg("pull")
        .current_dir(datadir)
        .output()
        .expect("failed to run git pull");

    println!("status: {}", output.status);
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}

fn run_git_push(args: &Args) {
    let datadir = get_datadir(args);
    let output = Command::new("git")
        .arg("push")
        .current_dir(datadir)
        .output()
        .expect("failed to run git push");

    println!("status: {}", output.status);
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}

fn run_git_sync(args: &Args) {
    run_git_pull(args);
    run_git_push(args);
}

fn run_git_init(args: &Args) {
    let datadir = get_datadir(args);
    match args.value_of("remote") {
        Some(remote) => {
            Repository::clone(remote, datadir).unwrap();
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
                msg,
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
    let datadir_clone_clone_clone = datadir.clone();

    let mut router = Router::new();
    router.get("/",
               move |r: &mut Request| webapp_projects(r, datadir.clone()),
               "projects");
    router.get("/timeline",
               move |r: &mut Request| webapp_timeline(r, datadir_clone.clone()),
               "timeline");
    router.get("/notes/:project",
               move |r: &mut Request| webapp_notes(r, datadir_clone_clone.clone()),
               "notes");
    router.get("/show/entries/:project",
               move |r: &mut Request| webapp_notes(r, datadir_clone_clone_clone.clone()),
               "notes_legacy");

    let (logger_before, logger_after) = Logger::new(None);

    let mut chain = Chain::new(router);

    // Link logger_before as your first before middleware.
    chain.link_before(logger_before);

    // Link logger_after as your *last* after middleware.
    chain.link_after(logger_after);

    info!("Listening on {}", listen_address);
    Iron::new(chain).http(listen_address).unwrap();
}

fn webapp_timeline(_: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let project = ALL_PROJECTS;

    let out = format_or_cached_git(get_timeline, project, &datadir, "_timeline");

    let mut resp = Response::with((status::Ok, out));
    resp.headers.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])));
    Ok(resp)
}

fn webapp_notes(req: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let project_req =
        req.extensions.get::<Router>().unwrap().find("project").unwrap_or(ALL_PROJECTS);
    let project =
        percent_encoding::percent_decode(project_req.as_bytes()).decode_utf8_lossy().into_owned();

    debug!("project: {}", project);

    let out = match project.as_str() {
        ALL_PROJECTS => {
            format_or_cached_git(format_notes, project.as_str(), &datadir, project.as_str())
        }
        _ => format_or_cached_modified(format_notes, project.as_str(), &datadir, project.as_str()),
    };

    let mut resp = Response::with((status::Ok, out));
    resp.headers.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])));
    Ok(resp)
}

fn find_cache_file(project: &str) -> Option<PathBuf> {
    let xdg = BaseDirectories::new().unwrap();
    let mut cache_path = PathBuf::from("lablog");
    cache_path.push(normalize_project_path(project, "cache"));

    let cachefile = xdg.find_cache_file(&cache_path);
    debug!("found cachefile: {:#?} from project {}", cachefile, project);

    cachefile
}

fn place_cache_file(project: &str) -> PathBuf {
    let xdg = BaseDirectories::new().unwrap();
    let mut cache_path = PathBuf::from("lablog");
    cache_path.push(normalize_project_path(project, "cache"));
    let cachefile = xdg.place_cache_file(&cache_path).unwrap();

    debug!("put cachefile: {:#?} from project {}", cachefile, project);

    cachefile
}

fn format_or_cached_modified(format: fn(&str, &PathBuf) -> String,
                             project: &str,
                             datadir: &PathBuf,
                             cachefile: &str)
                             -> String {
    match find_cache_file(cachefile) {
        None => {
            debug!("no cachefile will generate new file");
            get_projects_asiidoc_write_cache(format, project, datadir, &place_cache_file(cachefile))
        }
        Some(path) => {
            let data = file_to_string(&path).unwrap();
            let decoded: HTMLCache = json::decode(data.as_str()).unwrap();

            let mut project_path = datadir.clone();
            project_path.push(normalize_project_path(project, "csv"));
            trace!("project_path: {:#?}", project_path);

            let metadata = File::open(&project_path).unwrap().metadata().unwrap();
            let mtime = metadata.mtime();
            let mtime_nsec = metadata.mtime_nsec() as u32;

            trace!("mtime: {}", mtime);
            trace!("mtime_nsec: {}", mtime_nsec);

            let file_modified = NaiveDateTime::from_timestamp(mtime, mtime_nsec);

            trace!("file modified: {:#?}", file_modified);
            trace!("cache modified: {:#?}", decoded.modified);

            if file_modified <= decoded.modified {
                debug!("serve cachefile");
                decoded.html
            } else {
                debug!("file_modified is newer than decoded.modified");
                get_projects_asiidoc_write_cache(format, project, datadir, &path)
            }
        }
    }
}

fn format_or_cached_git(format: fn(&str, &PathBuf) -> String,
                        project: &str,
                        datadir: &PathBuf,
                        cachefile: &str)
                        -> String {
    match find_cache_file(cachefile) {
        None => {
            debug!("no cachefile will generate new file");
            get_projects_asiidoc_write_cache(format, project, datadir, &place_cache_file(cachefile))
        }
        Some(path) => {
            let data = file_to_string(&path).unwrap();
            let decoded: HTMLCache = json::decode(data.as_str()).unwrap();

            let gitcommit = githelper::get_current_commitid_for_repo(datadir).unwrap();
            if gitcommit == decoded.gitcommit {
                debug!("serve cachefile");
                decoded.html
            } else {
                debug!("commits different will generate new file");
                get_projects_asiidoc_write_cache(format, project, datadir, &path)
            }
        }
    }
}

fn format_notes(project: &str, datadir: &PathBuf) -> String {
    let projects = projects_or_project(project, datadir);
    let notes = get_projects_notes(datadir, &projects);

    format_projects_notes(notes)
}

fn get_projects_asiidoc_write_cache(format: fn(&str, &PathBuf) -> String,
                                    project: &str,
                                    datadir: &PathBuf,
                                    cachefile: &PathBuf)
                                    -> String {
    let asciidoc = format(project, datadir);
    let out = format_asciidoc(asciidoc);


    let mut project_path = datadir.clone();
    project_path.push(normalize_project_path(project, "csv"));

    let (mtime, mtime_nsec) = match File::open(&project_path) {
        Ok(file) => {
            match file.metadata() {
                Ok(metadata) => (metadata.mtime(), metadata.mtime_nsec() as u32),
                Err(_) => (0, 0),
            }
        }
        Err(_) => (0, 0),
    };

    debug!("project file mtime: {}", mtime);
    debug!("project file mtime_nsec: {}", mtime_nsec);

    let modified = NaiveDateTime::from_timestamp(mtime, mtime_nsec);

    debug!("cachefile put: {:#?}", cachefile);

    let cache = HTMLCache {
        gitcommit: githelper::get_current_commitid_for_repo(datadir).unwrap(),
        html: out.clone(),
        modified: modified,
    };

    let encoded = json::encode(&cache).unwrap();
    let mut file = File::create(cachefile).unwrap();
    file.write_all(encoded.as_bytes()).unwrap();

    out
}

fn format_asciidoc(input: String) -> String {
    let tmpdir = TempDir::new("lablog_tmp").unwrap();
    let tmppath = tmpdir.path().join("output.asciidoc");
    let mut file = File::create(&tmppath).unwrap();
    file.write_all(input.as_bytes()).unwrap();

    trace!("tmp file for asciidoc: {}",
           file_to_string(&tmppath).unwrap());

    let output = Command::new("asciidoctor")
        .arg("--out-file")
        .arg("-")
        .arg(tmppath)
        .output()
        .unwrap();

    debug!("status: {}", output.status);
    debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    debug!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn projects_or_project(project: &str, datadir: &PathBuf) -> DataSet<String> {
    match project {
        ALL_PROJECTS => get_projects(datadir),
        _ => {
            let mut out = DataSet::default();
            out.insert(project.into());
            out
        }
    }
}

fn format_projects_notes(notes: DataMap<&str, DataMap<DateTime<UTC>, Note>>) -> String {
    let mut out = String::new();

    let header = include_str!("notes.header.asciidoc");
    out.push_str(header);

    let indentreg = Regex::new(r"(?m)^=").unwrap();
    let indentrepl = "====";

    for (project, notes) in notes {
        out.push_str(format!("== {}\n", project).as_str());
        for (time_stamp, note) in notes {
            let indentnote = indentreg.replace_all(note.value.as_str(), indentrepl);
            out.push_str(format!("=== {}\n{}\n\n", time_stamp, indentnote).as_str())
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
                    tr {
                            td {
                                a(href="/notes/_") {
                                    : "All projects"
                                }
                            }
                    }
                    tr {
                        td {
                            a(href="/timeline/") {
                                : "Timeline"
                            }
                        }
                    }
                }
                hr{}
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
    let project = args.value_of("project").unwrap();

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

    debug!("project: {}", project);
    debug!("text: {}", text);

    let note = Note {
        time_stamp: UTC::now().into(),
        value: text.into(),
    };

    if write_note(&datadir, project, &note).is_some() {
        git_commit_note(&datadir, project, &note)
    }
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

fn git_commit_note(datadir: &PathBuf, project: &str, note: &Note) {
    let project_path = normalize_project_path(project, "csv");

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

    let before_notes = match args.value_of("filter_before") {
        Some(filter) => {
            debug!("filter notes before timestamp {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from parameter");
            debug!("filter notes before timestamp parsed: {:#?}", timestamp);

            filter_notes_by_timestamp(project_notes, timestamp, true)
        }
        None => project_notes,
    };

    let after_notes = match args.value_of("filter_after") {
        Some(filter) => {
            debug!("filter notes after {}", filter);
            let timestamp = try_multiple_time_parser(filter)
                .expect("can not parse timestamp from after parameter");
            debug!("filter notes before timestamp: {:#?}", timestamp);

            filter_notes_by_timestamp(before_notes, timestamp, false)
        }
        None => before_notes,
    };

    println!("{}", format_projects_notes(after_notes));
}

fn filter_notes_by_timestamp(notes: DataMap<&str, DataMap<DateTime<UTC>, Note>>,
                             timestamp: DateTime<UTC>,
                             before: bool)
                             -> DataMap<&str, DataMap<DateTime<UTC>, Note>> {

    let mut filtered_notes = DataMap::default();
    for (project, notes) in notes {
        let filternotes: DataMap<_, _> = notes.into_iter()
            .filter(|&(note_timestamp, _)| if before {
                note_timestamp >= timestamp
            } else {
                note_timestamp <= timestamp
            })
            .collect();

        if filternotes.is_empty() {
            filtered_notes.insert(project, filternotes);
        }
    }

    debug!("filtered_notes_before: {:#?}", filtered_notes);

    filtered_notes
}

fn try_multiple_time_parser(input: &str) -> ParseResult<DateTime<UTC>> {
    UTC.datetime_from_str(format!("{} 00:00:00", input).as_str(), "%Y-%m-%d %H:%M:%S")
}

fn get_projects_notes<'a>(datadir: &PathBuf,
                          projects: &'a DataSet<String>)
                          -> DataMap<&'a str, DataMap<DateTime<UTC>, Note>> {
    let mut map = DataMap::default();

    for project in projects.iter().map(|x| x.as_str()) {
        let mut project_path = datadir.clone();
        project_path.push(normalize_project_path(project, "csv"));

        let notes = get_notes(project_path);
        map.insert(project, notes);
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
        .filter(|e| !e.to_str().unwrap().starts_with('.'))
        .map(|e| e.with_extension(""))
        .collect();

    trace!("stripped_paths: {:#?}", stripped_paths);

    let projects = stripped_paths.into_iter()
        .map(|e| e.to_str().unwrap().replace("/", PROJECT_SEPPERATOR))
        .collect();

    trace!("projects: {:#?}", projects);

    projects
}

fn get_notes(project_path: PathBuf) -> DataMap<DateTime<UTC>, Note> {
    let mut map = DataMap::default();
    let mut rdr = csv::Reader::from_file(project_path).unwrap().has_headers(false);

    for record in rdr.decode() {
        let note: Note = record.unwrap();

        trace!("note: {:#?}", note);

        map.insert(note.time_stamp, note);
    }

    map
}

fn normalize_project_path(project: &str, extention: &str) -> String {
    format!("{}.{}",
            project.replace(PROJECT_SEPPERATOR, "/").as_str(),
            extention)
}

fn write_note(datadir: &PathBuf, project: &str, note: &Note) -> Option<()> {
    if note.value == "" {
        warn!("Note with empty value");
        return None;
    }

    let mut project_path = datadir.clone();
    project_path.push(normalize_project_path(project, "csv"));

    trace!("project_path: {:#?}", project_path);
    fs::create_dir_all(project_path.parent().unwrap()).unwrap();

    let mut file = match OpenOptions::new().append(true).open(&project_path) {
        Ok(file) => file,
        Err(_) => OpenOptions::new().append(true).create(true).open(&project_path).unwrap(),
    };

    let mut wtr = csv::Writer::from_memory();
    wtr.encode(note).unwrap();
    file.write_fmt(format_args!("{}", wtr.as_string())).unwrap();

    Some(())
}

#[derive(Debug,RustcEncodable,RustcDecodable)]
struct Note {
    time_stamp: DateTime<UTC>,
    value: String,
}

fn file_to_string(filepath: &Path) -> IOResult<String> {
    let mut s = String::new();
    let mut f = try!(File::open(filepath));
    try!(f.read_to_string(&mut s));

    Ok(s)
}

#[derive(Debug,RustcEncodable,RustcDecodable)]
struct HTMLCache {
    gitcommit: String,
    html: String,
    modified: NaiveDateTime,
}
