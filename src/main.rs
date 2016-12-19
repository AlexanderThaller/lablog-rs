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
use std::cmp::Ordering;
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

#[derive(Debug,RustcEncodable,RustcDecodable,Eq)]
struct Note {
    time_stamp: DateTime<UTC>,
    value: String,
}

impl Ord for Note {
    fn cmp(&self, other: &Note) -> Ordering {
        self.time_stamp.cmp(&other.time_stamp)
    }
}

impl PartialOrd for Note {
    fn partial_cmp(&self, other: &Note) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Note {
    fn eq(&self, other: &Note) -> bool {
        self.time_stamp == other.time_stamp
    }
}

type Notes = DataSet<Note>;

type Project<'a> = Option<&'a str>;
type Projects = DataSet<String>;

type ProjectsNotes = DataMap<String, Notes>;

#[derive(Debug,RustcEncodable,RustcDecodable)]
struct HTMLCache {
    gitcommit: String,
    html: String,
    modified: NaiveDateTime,
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

    trace!("app: {:#?}", app);
    trace!("matches: {:#?}", matches);

    run(app)
}

fn run(args: Args) {
    match args.subcommand.clone() {
        Some(command) => {
            match command.name.as_str() {
                "dates" => run_dates(command.matches),
                "note" => run_note(command.matches),
                "notes" => run_notes(command.matches),
                "timestamps" => run_timestamps(command.matches),
                "projects" => run_projects(command.matches),
                "repo" => run_repo(command.matches),
                "search" => run_search(command.matches),
                "timeline" => run_timeline(command.matches),
                "web" => run_web(command.matches),
                _ => {
                    error!("do not know what to do with this command: {}",
                           command.name.as_str())
                }
            }
        }
        None => run_projects(args),
    }
}

fn run_dates(args: Args) {
    let notes = get_filtered_notes(&args);

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

fn run_note(args: Args) {
    trace!("run_note args: {:#?}", args);
    let datadir = get_datadir(&args);
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

    let note = Note {
        time_stamp: UTC::now().into(),
        value: text.into(),
    };

    if write_note(&datadir, project, &note).is_some() {
        git_commit_note(&datadir, project, &note)
    }
}

fn run_notes(args: Args) {
    let notes = get_filtered_notes(&args);
    println!("{}", format_projects_notes(notes));
}

fn run_timestamps(args: Args) {
    let notes = get_filtered_notes(&args);

    for (project, notes) in notes {
        println!("{}", project);

        for (i, note) in notes.iter().enumerate() {
            println!("{}\t{}", i, note.time_stamp)
        }

        println!("");
    }
}

fn run_projects(args: Args) {
    trace!("run_projects args: {:#?}", args);
    let datadir = get_datadir(&args);

    let projects = get_projects(&datadir, args.value_of("project"));
    trace!("projects: {:#?}", projects);

    for project in projects {
        println!("{}", project)
    }
}

fn run_repo(args: Args) {
    if let Some(command) = args.subcommand.clone() {
        match command.name.as_str() {
            "init" => run_repo_init(&command.matches),
            "pull" => run_repo_pull(&command.matches),
            "push" => run_repo_push(&command.matches),
            "sync" => run_repo_sync(&command.matches),
            _ => {
                error!("do not know what to do with this command: {}",
                       command.name.as_str())
            }
        }
    }
}

fn run_repo_pull(args: &Args) {
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

fn run_repo_push(args: &Args) {
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

fn run_repo_sync(args: &Args) {
    run_repo_pull(args);
    run_repo_push(args);
}

fn run_repo_init(args: &Args) {
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

fn run_search(args: Args) {
    let text = args.value_of("text").unwrap();
    let notes = get_filtered_notes(&args);

    let re = Regex::new(text).unwrap();

    let mut searched = DataMap::default();
    for (project, notes) in notes {
        for note in notes {
            for line in note.value.lines() {
                if re.is_match(line) {
                    debug!("match on line: {}", line);

                    let find = re.find(line).unwrap();

                    debug!("find: {:#?}", find);

                    let replaced = String::from(line);
                    // Disabled until
                    // https://doc.rust-lang.org/std/string/struct.String.html#method.insert_str is
                    // in stable
                    // let mut replaced = String::from(line);
                    // replaced.insert_str(find.1, "\x1B[0m\x1B[0m");
                    // replaced.insert_str(find.0, "\x1B[1m\x1B[31m");

                    debug!("replaced: {}", replaced);

                    searched.entry(project.clone())
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
    let notes = get_filtered_notes(&args);
    println!("{}", get_timeline_for_notes(notes));
}

fn run_web(args: Args) {
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

fn git_commit_note(datadir: &PathBuf, project: Project, note: &Note) {
    let project_path = normalize_project_path(project, "csv");

    let repo = Repository::open(&datadir).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(project_path.as_str())).unwrap();
    index.write().unwrap();

    let commit_message = format!("{} - {} - added",
                                 note.time_stamp,
                                 project.expect("can not write commit message for the all projects \
                                              project"));
    git_commit(&repo, commit_message.as_str()).unwrap();
}

fn webapp_projects(_: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let projects = get_projects(&datadir, None);

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

fn webapp_timeline(_: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let out = format_or_cached_git(get_timeline, None, &datadir, Some("_timeline"));

    let mut resp = Response::with((status::Ok, out));
    resp.headers.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])));
    Ok(resp)
}

fn webapp_notes(req: &mut Request, datadir: std::path::PathBuf) -> IronResult<Response> {
    let project_req = req.extensions.get::<Router>().unwrap().find("project").unwrap_or("_");
    let project =
        percent_encoding::percent_decode(project_req.as_bytes()).decode_utf8_lossy().into_owned();

    debug!("project: {}", project);

    let out = match project.as_str() {
        "_" => format_or_cached_git(format_notes, None, &datadir, Some("_notes")),
        _ => {
            format_or_cached_modified(format_notes,
                                      Some(project.as_str()),
                                      &datadir,
                                      Some(project.as_str()))
        }
    };

    let mut resp = Response::with((status::Ok, out));
    resp.headers.set(ContentType(Mime(TopLevel::Text, SubLevel::Html, vec![])));
    Ok(resp)
}

fn cache_find_file(project: Project) -> Option<PathBuf> {
    let xdg = BaseDirectories::new().unwrap();
    let mut cache_path = PathBuf::from("lablog");
    cache_path.push(normalize_project_path(project, "cache"));

    let cachefile = xdg.find_cache_file(&cache_path);
    debug!("found cachefile: {:#?} from project {:#?}",
           cachefile,
           project);

    cachefile
}

fn cache_place_file(project: Project) -> PathBuf {
    let xdg = BaseDirectories::new().unwrap();
    let mut cache_path = PathBuf::from("lablog");
    cache_path.push(normalize_project_path(project, "cache"));
    let cachefile = xdg.place_cache_file(&cache_path).unwrap();

    debug!("put cachefile: {:#?} from project {:#?}",
           cachefile,
           project);

    cachefile
}

fn format_or_cached_modified(format: fn(Project, &PathBuf) -> String,
                             project: Project,
                             datadir: &PathBuf,
                             cachefile: Project)
                             -> String {
    match cache_find_file(cachefile) {
        None => {
            debug!("no cachefile will generate new file");
            get_projects_asiidoc_write_cache(format, project, datadir, &cache_place_file(cachefile))
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

fn format_or_cached_git(format: fn(Project, &PathBuf) -> String,
                        project: Project,
                        datadir: &PathBuf,
                        cachefile: Project)
                        -> String {
    match cache_find_file(cachefile) {
        None => {
            debug!("no cachefile will generate new file");
            get_projects_asiidoc_write_cache(format, project, datadir, &cache_place_file(cachefile))
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

fn format_notes(project: Option<&str>, datadir: &PathBuf) -> String {
    let projects = get_projects(datadir, project);
    let notes = get_projects_notes(datadir, projects);

    format_projects_notes(notes)
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

fn format_projects_notes(notes: ProjectsNotes) -> String {
    let mut out = String::new();

    let header = include_str!("notes.header.asciidoc");
    out.push_str(header);

    let indentreg = Regex::new(r"(?m)^=").unwrap();
    let indentrepl = "====";

    for (project, notes) in notes {
        out.push_str(format!("== {}\n", project).as_str());
        for note in notes {
            let indentnote = indentreg.replace_all(note.value.as_str(), indentrepl);
            out.push_str(format!("=== {}\n{}\n\n", note.time_stamp, indentnote).as_str())
        }
    }

    out
}

fn get_timeline_for_notes(notes: ProjectsNotes) -> String {
    let mut timeline = DataMap::default();
    for (project, notes) in notes {
        for note in notes {
            timeline.entry(note.time_stamp.date())
                .or_insert_with(DataMap::default)
                .entry(project.clone())
                .or_insert_with(DataMap::default)
                .insert(note.time_stamp, note);
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

fn get_timeline(project: Option<&str>, datadir: &PathBuf) -> String {
    let projects = get_projects(datadir, project);
    let project_notes = get_projects_notes(datadir, projects);

    get_timeline_for_notes(project_notes)
}

fn get_filtered_notes(args: &Args) -> ProjectsNotes {
    let datadir = get_datadir(args);
    let projects = get_projects(&datadir, args.value_of("project"));
    let project_notes = get_projects_notes(&datadir, projects);

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

fn get_projects_asiidoc_write_cache(format: fn(Project, &PathBuf) -> String,
                                    project: Project,
                                    datadir: &PathBuf,
                                    cachefile: &PathBuf)
                                    -> String {
    let asciidoc = format(project, datadir);
    let out = format_asciidoc(asciidoc);

    let modified = match project {
        Some(_) => {
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
            NaiveDateTime::from_timestamp(mtime, mtime_nsec)
        }
        None => NaiveDateTime::from_timestamp(0, 0),
    };

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

fn get_projects_notes(datadir: &PathBuf, projects: Projects) -> ProjectsNotes {
    let mut map = DataMap::default();

    for project in projects {
        let mut project_path = datadir.clone();
        project_path.push(normalize_project_path(Some(project.as_str()), "csv"));

        let notes = get_notes(project_path);
        map.insert(project, notes);
    }

    map
}

fn get_projects(datadir: &PathBuf, project: Project) -> Projects {
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

    let projects: Projects = stripped_paths.into_iter()
        .map(|e| e.to_str().unwrap().replace("/", PROJECT_SEPPERATOR))
        .collect();

    trace!("projects: {:#?}", projects);

    match project {
        Some(project) => {
            let re = Regex::new(project).unwrap();
            projects.into_iter().filter(|project| re.is_match(project)).collect()
        }
        None => projects,
    }
}

fn get_notes(project_path: PathBuf) -> Notes {
    let mut map = DataSet::default();
    let mut rdr = csv::Reader::from_file(project_path).unwrap().has_headers(false);

    for record in rdr.decode() {
        let note: Note = record.unwrap();

        trace!("note: {:#?}", note);

        map.insert(note);
    }

    map
}

fn filter_notes_by_timestamp(notes: ProjectsNotes,
                             timestamp: DateTime<UTC>,
                             after: bool)
                             -> ProjectsNotes {
    trace!("notes before filter: {:#?}", notes);

    let mut filtered_notes = DataMap::default();
    for (project, notes) in notes {
        let filternotes: DataSet<_> = notes.into_iter()
            .filter(|note| {
                trace!("filter note: {:#?}", note);
                trace!("timestamp: {:#?}", timestamp);

                let yield_note = if after {
                    debug!("filter after");
                    note.time_stamp >= timestamp
                } else {
                    debug!("filter before");
                    note.time_stamp <= timestamp
                };

                trace!("yield: {}", yield_note);

                yield_note
            })
            .collect();

        if !filternotes.is_empty() {
            debug!("filternotes is not empty");
            filtered_notes.insert(project, filternotes);
        }
    }

    trace!("notes after filter: {:#?}", filtered_notes);

    filtered_notes
}

fn write_note(datadir: &PathBuf, project: Project, note: &Note) -> Option<()> {
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

fn try_multiple_time_parser(input: &str) -> ParseResult<DateTime<UTC>> {
    let input = match input {
        "today" => format!("{}", Local::now().format("%Y-%m-%d")),
        "yesterday" => {
            let yesterday = Local::now() - Duration::days(1);
            format!("{}", yesterday.format("%Y-%m-%d"))
        }
        _ => String::from(input),
    };

    trace!("time_parser input after natural timestamp: {}", input);

    input.parse()
        .or(UTC.datetime_from_str(input.as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}:00", input).as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}:00:00", input).as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{} 00:00:00", input).as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}-01 00:00:00", input).as_str(),
                                  "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}-01-01 00:00:00", input).as_str(),
                                  "%Y-%m-%d %H:%M:%S"))
}

fn normalize_project_path(project: Project, extention: &str) -> String {
    match project {
        Some(project) => {
            format!("{}.{}",
                    project.replace(PROJECT_SEPPERATOR, "/").as_str(),
                    extention)
        }
        None => panic!("can not normalize the all projects project"),
    }
}

fn file_to_string(filepath: &Path) -> IOResult<String> {
    let mut s = String::new();
    let mut f = File::open(filepath)?;
    f.read_to_string(&mut s)?;

    Ok(s)
}
