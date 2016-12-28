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
#![feature(plugin)]
#![plugin(rocket_codegen)]
extern crate rocket;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate log;

#[macro_use]
extern crate horrorshow;

extern crate chrono;
extern crate csv;
extern crate env_logger;
extern crate githelper;
extern crate libc;
extern crate loggerv;
extern crate regex;
extern crate rustc_serialize;
extern crate tempdir;
extern crate walkdir;
extern crate xdg;

use chrono::*;
use clap::App;
use clap::ArgMatches as Args;
use horrorshow::prelude::*;
use log::LogLevel;
use regex::Regex;
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
use walkdir::WalkDir;
use xdg::BaseDirectories;
use rocket::request::Form;
use rocket::response::Redirect;

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
                "web" => run_webapp(command.matches),
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

    let timestamp: DateTime<UTC> = match args.value_of("timestamp") {
        None => UTC::now().into(),
        Some(ts) => ts.parse().unwrap(),
    };

    let note = Note {
        time_stamp: timestamp,
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
    githelper::pull(datadir.as_path()).expect("can not pull from repo");
}

fn run_repo_push(args: &Args) {
    let datadir = get_datadir(args);
    githelper::push(datadir.as_path()).expect("can not push to repo");
}

fn run_repo_sync(args: &Args) {
    let datadir = get_datadir(args);
    githelper::sync(datadir.as_path()).expect("can not sync repo");
}

fn run_repo_init(args: &Args) {
    let datadir = get_datadir(args);
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

fn run_webapp(_: Args) {
    rocket::ignite()
        .mount("/",
               routes![webapp_index,
                       webapp_timeline,
                       webapp_notes,
                       webapp_notes_legacy,
                       webapp_note,
                       webapp_note_add])
        .launch();
}

#[get("/")]
fn webapp_index() -> String {
    let datadir = get_datadir2();
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
                    tr {
                        td {
                            a(href="/note/") {
                                : "Add Note"
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

    out
}

#[get("/timeline")]
fn webapp_timeline() -> String {
    let datadir = get_datadir2();
    format_or_cached_git(get_timeline, None, &datadir, Some("_timeline"))
}

#[get("/notes/<project>")]
fn webapp_notes(project: &str) -> String {
    let datadir = get_datadir2();
    match project {
        "_" => format_or_cached_git(format_notes, None, &datadir, Some("_notes")),
        _ => format_or_cached_modified(format_notes, Some(project), &datadir, Some(project)),
    }
}

#[get("/show/entries/<project>")]
fn webapp_notes_legacy(project: &str) -> String {
    let datadir = get_datadir2();
    match project {
        "_" => format_or_cached_git(format_notes, None, &datadir, Some("_notes")),
        _ => format_or_cached_modified(format_notes, Some(project), &datadir, Some(project)),
    }
}

#[get("/note")]
fn webapp_note() -> String {
    let datadir = get_datadir2();
    let projects = get_projects(&datadir, None);

    let out = html!{
    html(lang="en") {
        head {
            title { : "Lablog - Add Note" }
        }
        meta(charset="utf-8"){}
        body {
            a(href="/"){ : "Back" }
            hr{}

            h1 { : "Note" }

            form(action="/note_add", method="post") {
                input(name="project", list="projects", style="width: 100%"){}

                datalist(id="projects"){
                    @ for project in projects {
                        option(value=format_args!("{}", project)) {}
                    }
                }

                br{}

                textarea(name="note", style="width: 100%; height: 500px") {}

                br{}

                input(type="submit", style="width: 150px; height: 30px")
            }
        }
    }}
        .into_string()
        .unwrap();

    out
}

#[derive(FromForm,Debug)]
struct NotesForm {
    project: String,
    note: String,
}

impl<'a> Into<Note> for &'a NotesForm {
    fn into(self) -> Note {
        Note {
            time_stamp: UTC::now().into(),
            value: self.note.clone(),
        }
    }
}

#[post("/note_add", data="<noteform>")]
fn webapp_note_add(noteform: Form<NotesForm>) -> Redirect {
    let note: Note = noteform.get().into();
    let project = noteform.get().project.as_str();


    let datadir = get_datadir2();
    if write_note(&datadir, Some(project), &note).is_some() {
        git_commit_note(&datadir, Some(project), &note)
    }

    githelper::sync(datadir.as_path()).expect("can not sync repo");

    let timestamp = asciidoc_timestamp(format!("{}", note.time_stamp).as_str());
    Redirect::to(format!("/notes/{}#{}", project, timestamp).as_str())
}

fn asciidoc_timestamp(input: &str) -> String {
    String::from(input).to_lowercase().replace(' ', "-").replace(':', "-").replace('.', "-")
}

#[test]
fn test_asciidoc_timestamp() {
    assert_eq!(asciidoc_timestamp("2016-10-27 15:14:07.704374171 UTC"),
               "2016-10-27-15-14-07-704374171-utc")

}

fn git_commit_note(datadir: &PathBuf, project: Project, note: &Note) {
    let project_path = normalize_project_path(project, "csv");

    githelper::add(&datadir, Path::new(project_path.as_str()))
        .expect("can not add project file changes to git");

    let commit_message = format!("{} - {} - added",
                                 note.time_stamp,
                                 project.expect("can not write commit message for the all projects \
                                              project"));
    githelper::commit(&datadir, commit_message.as_str()).expect("can not commit note to repo");
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

fn get_datadir2() -> PathBuf {
    let xdg = BaseDirectories::new().unwrap();
    xdg.create_data_directory("lablog").unwrap()
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
