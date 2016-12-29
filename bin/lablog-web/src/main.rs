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
use rocket_contrib::Template;
use rocket::request::Form;
use rocket::response::Redirect;
use rustc_serialize::json;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
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

#[derive(FromForm,Debug)]
struct NotesForm {
    project: String,
    note: String,
}

fn main() {
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
fn webapp_index() -> Template {
    let datadir = get_datadir2();
    let projects = get_projects(&datadir, None);
    let context = IndexContext { projects: projects };

    Template::render("index", &context)
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
fn webapp_note() -> Template {
    let datadir = get_datadir2();
    let projects = get_projects(&datadir, None);
    let context = IndexContext { projects: projects };

    Template::render("note", &context)
}

#[post("/note_add", data="<noteform>")]
fn webapp_note_add(noteform: Form<NotesForm>) -> Redirect {
    let note: Note = note_from_form(noteform.get());
    let project = noteform.get().project.as_str();

    let datadir = get_datadir2();
    if write_note(&datadir, Some(project), &note).is_some() {
        git_commit_note(&datadir, Some(project), &note)
    }

    if let Err(err) = githelper::sync(datadir.as_path()) {
        warn!("can not sync with repo: {}", err)
    }

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

fn get_datadir2() -> PathBuf {
    let xdg = BaseDirectories::new().unwrap();
    xdg.create_data_directory("lablog").unwrap()
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

fn note_from_form(form: &NotesForm) -> Note {
    Note {
        time_stamp: UTC::now().into(),
        value: form.note.clone(),
    }
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