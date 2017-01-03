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
#![feature(proc_macro)]
#![plugin(rocket_codegen)]

extern crate chrono;
extern crate githelper;
extern crate lablog_lib;
extern crate rocket;
extern crate rocket_contrib;
extern crate rustc_serialize;
extern crate tempdir;
extern crate xdg;

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

use chrono::*;
use lablog_lib::file_to_string;
use lablog_lib::format_notes;
use lablog_lib::get_projects;
use lablog_lib::get_timeline;
use lablog_lib::git_commit_note;
use lablog_lib::normalize_project_path;
use lablog_lib::Note;
use lablog_lib::Project;
use lablog_lib::Projects;
use lablog_lib::write_note;
use rocket::config;
use rocket_contrib::Template;
use rocket::request::Form;
use rocket::response::NamedFile;
use rocket::response::Redirect;
use rustc_serialize::json;
use std::fs::File;
use std::io;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempdir::TempDir;
use xdg::BaseDirectories;

#[derive(Debug,RustcEncodable,RustcDecodable)]
struct HTMLCache {
    gitcommit: String,
    html: String,
    modified: NaiveDateTime,
}

#[derive(Serialize)]
struct IndexContext {
    projects: Projects,
}

#[derive(Serialize,Debug)]
struct NoteContext {
    projects: Projects,
    selected_project: Option<String>,
}

#[derive(Serialize,Debug)]
struct NotesContext {
    add_note: Option<String>,
    formatted_notes: String,
    parent: Option<String>,
    children: Option<Projects>,
    title: String,
}

#[derive(FromForm,Debug)]
struct NotesForm {
    project: String,
    note: String,
}

#[derive(FromForm,Debug,Default)]
struct PageOptions {
    raw: Option<bool>,
}

fn main() {
    rocket::ignite()
        .mount("/",
               routes![webapp_index,
                       webapp_timeline,
                       webapp_notes_all,
                       webapp_notes,
                       webapp_notes_legacy,
                       webapp_note,
                       webapp_note_project,
                       webapp_note_add,
                       webapp_static])
        .launch();
}

#[get("/")]
fn webapp_index() -> Template {
    let datadir = get_datadir();
    let projects = get_projects(&datadir, None);
    let context = IndexContext { projects: projects };

    Template::render("index", &context)
}

#[get("/timeline")]
fn webapp_timeline() -> Template {
    let datadir = get_datadir();
    let formatted = format_or_cached_git(get_timeline,
                                         None,
                                         &datadir,
                                         Some(String::from("_timeline")),
                                         false);

    let context = NotesContext {
        add_note: None,
        children: None,
        formatted_notes: formatted,
        parent: None,
        title: String::from("Timeline"),
    };

    Template::render("notes", &context)
}

#[get("/notes")]
fn webapp_notes_all() -> Template {
    let datadir = get_datadir();
    let formatted = format_or_cached_git(format_notes,
                                         None,
                                         &datadir,
                                         Some(String::from("_notes")),
                                         false);

    let context = NotesContext {
        add_note: None,
        children: None,
        formatted_notes: formatted,
        parent: None,
        title: String::from("All Notes"),
    };

    Template::render("notes", &context)
}

#[get("/notes/<project>")]
fn webapp_notes(project: String) -> Template {
    let datadir = get_datadir();
    // TODO: Find a way to cache notes based on modified date again which also works for aggregated
    // views like the athaller projects that doesnt have any notes by itself but children projects
    // with notes.
    let formatted = format_or_cached_git(format_notes,
                                         Some(project.clone()),
                                         &datadir,
                                         Some(project.clone()),
                                         false);

    let projects = get_projects(&datadir, Some(project.clone()));

    let context = NotesContext {
        add_note: Some(project.clone()),
        children: lablog_lib::get_children(projects, Some(project.clone())),
        formatted_notes: formatted,
        parent: lablog_lib::get_parent(Some(project.clone())),
        title: project.clone(),
    };

    Template::render("notes", &context)
}

#[get("/show/entries/<project>")]
fn webapp_notes_legacy(project: String) -> Template {
    let datadir = get_datadir();
    let formatted = match project.as_str() {
        "_" => {
            format_or_cached_git(format_notes,
                                 None,
                                 &datadir,
                                 Some(String::from("_notes")),
                                 false)
        }
        _ => {
            format_or_cached_git(format_notes,
                                 Some(project.clone()),
                                 &datadir,
                                 Some(project.clone()),
                                 false)
        }
    };

    let projects = get_projects(&datadir, Some(project.clone()));

    let context = NotesContext {
        children: match project.as_str() {
            "_" => None,
            _ => lablog_lib::get_children(projects, Some(project.clone())),
        },
        parent: match project.as_str() {
            "_" => None,
            _ => lablog_lib::get_parent(Some(project.clone())),
        },
        title: match project.as_str() {
            "_" => String::from("All Notes"),
            _ => project.clone(),
        },
        add_note: match project.as_str() {
            "_" => None,
            _ => Some(project.clone()),
        },
        formatted_notes: formatted,
    };

    Template::render("notes", &context)
}

#[get("/note")]
fn webapp_note() -> Template {
    let datadir = get_datadir();
    let projects = get_projects(&datadir, None);

    let context = NoteContext {
        projects: projects,
        selected_project: None,
    };

    Template::render("note", &context)
}

#[get("/note/<project>")]
fn webapp_note_project(project: String) -> Template {
    let datadir = get_datadir();
    let projects = get_projects(&datadir, None);

    let context = NoteContext {
        projects: projects,
        selected_project: Some(project.clone()),
    };

    Template::render("note", &context)
}

#[post("/note_add", data="<noteform>")]
fn webapp_note_add(noteform: Form<NotesForm>) -> Redirect {
    let note: Note = note_from_form(noteform.get());
    let project = noteform.get().project.as_str();

    let datadir = get_datadir();
    if write_note(&datadir, Some(String::from(project)), &note).is_some() {
        git_commit_note(&datadir, Some(String::from(project)), &note)
    }

    if let Err(err) = githelper::sync(datadir.as_path()) {
        warn!("can not sync with repo: {}", err)
    }

    let timestamp = asciidoc_timestamp(format!("{}", note.time_stamp).as_str());
    Redirect::to(format!("/notes/{}#{}", project, timestamp).as_str())
}

#[get("/<path..>", rank = 5)]
fn webapp_static(path: PathBuf) -> io::Result<NamedFile> {
    NamedFile::open(Path::new("static/").join(path))
}

fn asciidoc_timestamp(input: &str) -> String {
    String::from(input).to_lowercase().replace(' ', "-").replace(':', "-").replace('.', "-")
}

#[test]
fn test_asciidoc_timestamp() {
    assert_eq!(asciidoc_timestamp("2016-10-27 15:14:07.704374171 UTC"),
               "2016-10-27-15-14-07-704374171-utc")
}

fn get_datadir() -> PathBuf {
    let conf = config::active().expect("can not get config for reading the datadir");

    let datadir = match conf.get_str("datadir") {
        Ok(config) => PathBuf::from(config),
        Err(_) => {
            let xdg = BaseDirectories::new()
                .expect("can not create new xdg context for creating the datadir");
            xdg.create_data_directory("lablog").expect("can not create new datadir directory")
        }
    };

    debug!("datadir: {:#?}", datadir);

    datadir
}

fn format_or_cached_modified(format: fn(Project, &PathBuf) -> String,
                             project: Project,
                             datadir: &PathBuf,
                             cachefile: Project,
                             raw: bool)
                             -> String {
    if raw {
        return format(project, datadir);
    }

    match cache_find_file(cachefile.clone()) {
        None => {
            debug!("no cachefile will generate new file");
            get_projects_asiidoc_write_cache(format, project, datadir, &cache_place_file(cachefile))
        }
        Some(path) => {
            let data = file_to_string(&path).expect("can not read cache file");
            let decoded: HTMLCache = json::decode(data.as_str())
                .expect("can not decode cache file");

            let mut project_path = datadir.clone();
            project_path.push(normalize_project_path(project.clone(), "csv"));
            trace!("project_path: {:#?}", project_path);

            let metadata = File::open(&project_path)
                .expect("can not open file of cachefile to read metadata")
                .metadata()
                .expect("can not read metadata from cachefile");

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
                        cachefile: Project,
                        raw: bool)
                        -> String {
    if raw {
        return format(project, datadir);
    }

    match cache_find_file(cachefile.clone()) {
        None => {
            debug!("no cachefile will generate new file");
            get_projects_asiidoc_write_cache(format, project, datadir, &cache_place_file(cachefile))
        }
        Some(path) => {
            let data = file_to_string(&path).expect("can not read data from cached git file");
            let decoded: HTMLCache = json::decode(data.as_str())
                .expect("can not decode data of a git cache file");

            let gitcommit = githelper::get_current_commitid_for_repo(datadir)
                .expect("can not get the current git commit for a git cache file");
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

fn get_projects_asiidoc_write_cache(format: fn(Project, &PathBuf) -> String,
                                    project: Project,
                                    datadir: &PathBuf,
                                    cachefile: &PathBuf)
                                    -> String {
    let asciidoc = format(project.clone(), datadir);
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
        gitcommit: githelper::get_current_commitid_for_repo(datadir)
            .expect("can not get current commit for writing to cachefile"),
        html: out.clone(),
        modified: modified,
    };

    let encoded = json::encode(&cache).expect("can not encode cachefile");
    let mut file = File::create(cachefile).expect("can not create cachefile");
    file.write_all(encoded.as_bytes()).expect("can not write to cachefile");

    out
}

fn note_from_form(form: &NotesForm) -> Note {
    Note {
        time_stamp: UTC::now().into(),
        value: form.note.clone(),
    }
}

fn format_asciidoc(input: String) -> String {
    let tmpdir = TempDir::new("lablog-web_tmp")
        .expect("can not create a new tmpdir for asciiformatting");
    let tmppath = tmpdir.path().join("output.asciidoc");
    let mut file = File::create(&tmppath).expect("can not create a new file for asciiformatting");
    file.write_all(input.as_bytes()).expect("can not write to asciiformatting file");

    let output = Command::new("asciidoctor")
        .arg("--safe-mode")
        .arg("safe")
        .arg("--no-header-footer")
        .arg("--out-file")
        .arg("-")
        .arg(tmppath)
        .output()
        .expect("problems while running asciidoctor");

    debug!("status: {}", output.status);
    debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    debug!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn cache_find_file(project: Project) -> Option<PathBuf> {
    let xdg = BaseDirectories::new().expect("can not get new xdg context for finding a cachefile");
    let mut cache_path = PathBuf::from("lablog-web");
    cache_path.push(normalize_project_path(project.clone(), "cache"));

    let cachefile = xdg.find_cache_file(&cache_path);
    debug!("found cachefile: {:#?} from project {:#?}",
           cachefile,
           project);

    cachefile
}

fn cache_place_file(project: Project) -> PathBuf {
    let xdg = BaseDirectories::new()
        .expect("can not open a new xdg context for writing a cachefile");
    let mut cache_path = PathBuf::from("lablog-web");
    cache_path.push(normalize_project_path(project.clone(), "cache"));
    let cachefile = xdg.place_cache_file(&cache_path).expect("can not place a new cachefile");

    debug!("put cachefile: {:#?} from project {:#?}",
           cachefile,
           project);

    cachefile
}
