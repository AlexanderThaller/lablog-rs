extern crate chrono;
extern crate csv;
extern crate githelper;
extern crate regex;
extern crate rustc_serialize;
extern crate walkdir;

#[macro_use]
extern crate log;

use chrono::*;
use regex::Regex;
use std::cmp::Ordering;
use std::collections::BTreeMap as DataMap;
use std::collections::BTreeSet as DataSet;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Result as IOResult;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use walkdir::WalkDir;

const PROJECT_SEPPERATOR: &'static str = ".";

pub type Project = Option<String>;
pub type Projects = DataSet<String>;

pub type ProjectsNotes = DataMap<String, Notes>;
type Notes = DataSet<Note>;

#[derive(Debug,RustcEncodable,RustcDecodable,Eq,Clone)]
pub struct Note {
    pub time_stamp: DateTime<UTC>,
    pub value: String,
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

impl Default for Note {
    fn default() -> Note {
        Note {
            time_stamp: UTC::now(),
            value: String::new(),
        }
    }
}

pub fn git_commit_note(datadir: &PathBuf, project: Project, note: Option<&Note>, message: &str) {
    let project_path = normalize_project_path(project.clone(), "csv");

    githelper::add(datadir, Path::new(project_path.as_str()))
        .expect("can not add project file changes to git");

    let commit_message = match note {
        Some(note) => {
            format!("{} - {} - {}",
                    note.time_stamp,
                    project.expect("can not write commit message for the all projects project"),
                    message)
        }
        None => {
            format!("{} - {}",
                    project.expect("can not write commit message for the all projects project"),
                    message)
        }
    };

    githelper::commit(datadir, commit_message.as_str()).expect("can not commit note to repo");
}

pub fn normalize_project_path(project: Project, extention: &str) -> String {
    match project {
        Some(project) => {
            format!("{}.{}",
                    project.replace(PROJECT_SEPPERATOR, "/").as_str(),
                    extention)
        }
        None => panic!("can not normalize the all projects project"),
    }
}

pub fn write_project(datadir: &PathBuf,
                     project: Project,
                     notes: &Notes,
                     overwrite: bool)
                     -> Option<()> {
    let mut project_path = datadir.clone();
    project_path.push(normalize_project_path(project, "csv"));

    trace!("project_path: {:#?}", project_path);
    fs::create_dir_all(project_path.parent().unwrap()).unwrap();

    let mut file = if overwrite {
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&project_path)
            .unwrap()
    } else {
        match OpenOptions::new().append(true).open(&project_path) {
            Ok(file) => file,
            Err(_) => {
                OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&project_path)
                    .unwrap()
            }
        }
    };

    let mut wtr = csv::Writer::from_memory();
    for note in notes {
        wtr.serialize(note.time_stamp, note.value)
            .chain_err(|| "can not serialize note")?;
        file.write_fmt(format_args!("{}", wtr.as_string()))
            .unwrap();
    }

    Some(())
}

pub fn write_note(datadir: &PathBuf, project: Project, note: &Note) -> Option<()> {
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
        Err(_) => {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&project_path)
                .unwrap()
        }
    };

    let mut wtr = csv::Writer::from_memory();
    wtr.encode(note).unwrap();
    file.write_fmt(format_args!("{}", wtr.as_string()))
        .unwrap();

    Some(())
}

pub fn get_projects(datadir: &PathBuf, project: Project) -> Projects {
    let ok_walkdir: Vec<walkdir::DirEntry> = WalkDir::new(&datadir)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    trace!("ok_walkdir: {:#?}", ok_walkdir);

    let stripped_paths: Vec<PathBuf> = ok_walkdir
        .iter()
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

    let projects: Projects = stripped_paths
        .into_iter()
        .map(|e| e.to_str().unwrap().replace("/", PROJECT_SEPPERATOR))
        .collect();

    trace!("projects: {:#?}", projects);

    filter_projects(projects, project)
}

pub fn filter_projects(projects: Projects, project: Project) -> Projects {
    match project {
        Some(project) => {
            let re = Regex::new(project.as_str()).unwrap();
            projects
                .into_iter()
                .filter(|project| re.is_match(project))
                .collect()
        }
        None => projects,
    }
}

pub fn file_to_string(filepath: &Path) -> IOResult<String> {
    let mut s = String::new();
    let mut f = File::open(filepath)?;
    f.read_to_string(&mut s)?;

    Ok(s)
}

pub fn get_timeline(project: Project, datadir: &PathBuf) -> String {
    let projects = get_projects(datadir, project);
    let project_notes = get_projects_notes(datadir, projects);

    get_timeline_for_notes(project_notes)
}

pub fn get_timeline_for_notes(notes: ProjectsNotes) -> String {
    let mut timeline = DataMap::default();
    for (project, notes) in notes {
        for note in notes {
            timeline
                .entry(note.time_stamp.date())
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

pub fn get_projects_notes(datadir: &PathBuf, projects: Projects) -> ProjectsNotes {
    let mut map = DataMap::default();

    for project in projects {
        let mut project_path = datadir.clone();
        project_path.push(normalize_project_path(Some(project.clone()), "csv"));

        let notes = get_notes(project_path);
        map.insert(project, notes);
    }

    map
}

fn get_notes(project_path: PathBuf) -> Notes {
    let mut map = DataSet::default();
    let mut rdr = csv::Reader::from_file(project_path)
        .unwrap()
        .has_headers(false);

    for record in rdr.decode() {
        let note: Note = record.unwrap();

        trace!("note: {:#?}", note);

        map.insert(note);
    }

    map
}

pub fn format_notes(project: Project, datadir: &PathBuf) -> String {
    let projects = get_projects(datadir, project);
    let notes = get_projects_notes(datadir, projects);

    format_projects_notes(notes)
}

pub fn format_projects_notes(notes: ProjectsNotes) -> String {
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

pub fn filter_notes_by_timestamp(notes: ProjectsNotes,
                                 timestamp: DateTime<UTC>,
                                 after: bool)
                                 -> ProjectsNotes {
    trace!("notes before filter: {:#?}", notes);

    let mut filtered_notes = DataMap::default();
    for (project, notes) in notes {
        let filternotes: DataSet<_> = notes
            .into_iter()
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

pub fn try_multiple_time_parser(input: &str) -> ParseResult<DateTime<UTC>> {
    let input = match input {
        "today" => format!("{}", Local::now().format("%Y-%m-%d")),
        "yesterday" => {
            let yesterday = Local::now() - Duration::days(1);
            format!("{}", yesterday.format("%Y-%m-%d"))
        }
        _ => String::from(input),
    };

    trace!("time_parser input after natural timestamp: {}", input);

    input
        .parse()
        .or(UTC.datetime_from_str(input.as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}:00", input).as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}:00:00", input).as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{} 00:00:00", input).as_str(), "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}-01 00:00:00", input).as_str(),
                                  "%Y-%m-%d %H:%M:%S"))
        .or(UTC.datetime_from_str(format!("{}-01-01 00:00:00", input).as_str(),
                                  "%Y-%m-%d %H:%M:%S"))
}

pub fn get_parent(project: Project) -> Project {
    if project.is_none() {
        return None;
    }

    let unwrap = project.unwrap();
    let mut split: Vec<&str> = unwrap.split('.').collect();

    let len = split.len();
    if len == 0 {
        return None;
    }

    if len == 1 {
        return Some(String::from(""));
    }

    split.truncate(len - 1);

    Some(split.join("."))
}

#[test]
fn test_get_parent() {
    assert_eq!(None, get_parent(None));

    assert_eq!(Some(String::from("")),
               get_parent(Some(String::from("athaller"))));

    assert_eq!(Some(String::from("")), get_parent(Some(String::new())));

    assert_eq!(Some(String::from("athaller")),
               get_parent(Some(String::from("athaller.test"))));

    assert_eq!(Some(String::from("athaller.test")),
               get_parent(Some(String::from("athaller.test.test"))));

    assert_eq!(Some(String::from("")), get_parent(Some(String::from(""))));

    assert_eq!(Some(String::from("")), get_parent(Some(String::from("."))));

    assert_eq!(Some(String::from(".")),
               get_parent(Some(String::from(".."))));
}

pub fn get_children(projects: Projects, project: Project) -> Option<Projects> {
    let projects = match project {
        None => DataSet::new(),
        Some(project) => filter_projects(projects, Some(format!("^{}\\.[^.]*$", project))),
    };

    if projects.is_empty() {
        return None;
    }

    Some(projects)
}

#[test]
fn test_get_children() {
    let mut projects = DataSet::new();
    projects.insert(String::from("no_children"));

    projects.insert(String::from("children"));
    projects.insert(String::from("children.1"));
    projects.insert(String::from("children.2"));
    projects.insert(String::from("children.3"));
    projects.insert(String::from("children.3.1"));
    projects.insert(String::from("children.3.2"));
    projects.insert(String::from("children.3.3"));

    let mut children = DataSet::new();
    children.insert(String::from("children.1"));
    children.insert(String::from("children.2"));
    children.insert(String::from("children.3"));

    assert_eq!(None, get_children(projects.clone(), None));

    assert_eq!(None,
               get_children(projects.clone(), Some(String::from("no_children"))));

    assert_eq!(Some(children),
               get_children(projects.clone(), Some(String::from("children"))));
}

pub fn archive_project(datadir: &PathBuf, projects: Projects, project: Project, recursive: bool) {
    if recursive {
        let children = get_children(projects.clone(), project.clone());

        if children.is_some() {
            for project in children.unwrap() {
                archive_project(datadir, projects.clone(), Some(project), recursive);
            }
        }
    }

    let mut old_path = datadir.clone();
    old_path.push(normalize_project_path(project.clone(), "csv"));

    if !old_path.exists() {
        return;
    }

    let new_path = get_archive_path(datadir, project.clone());
    fs::create_dir_all(new_path
                           .parent()
                           .expect("can not get parent directory of archive path"))
            .expect("can not create folder for archive path");

    let mut file = match OpenOptions::new().append(true).open(&new_path) {
        Ok(file) => file,
        Err(_) => {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&new_path)
                .expect("can not open archive file")
        }
    };

    let old_data = file_to_string(&old_path).expect("can not read data from project for archiving");

    file.write_all(old_data.as_bytes())
        .expect("can not write to archive file");

    fs::remove_file(old_path.clone()).expect("can not remove old file after archiving");

    githelper::add(datadir, Path::new(new_path.as_path()))
        .expect("can not add project archive file to git");

    githelper::add(datadir, Path::new(old_path.as_path()))
        .expect("can not add removed project file to git");

    let commit_message = format!("{} - {} - archived", Local::now(), project.unwrap());

    githelper::commit(datadir, commit_message.as_str())
        .expect("can not commit archive move to repo");
}

fn get_archive_path(datadir: &PathBuf, project: Project) -> PathBuf {
    let norm_path = normalize_project_path(project, "csv");

    let mut archive_path = datadir.clone();
    archive_path.push(".archive");
    archive_path.push(norm_path);

    archive_path
}

#[test]
fn test_get_archive_path() {
    let datadir = Path::new("/tmp").to_path_buf();

    {
        let project = Some(String::from("test"));
        let expected = Path::new("/tmp/.archive/test.csv").to_path_buf();

        assert_eq!(expected, get_archive_path(&datadir, project));
    }

    {
        let project = Some(String::from("test.test2"));
        let expected = Path::new("/tmp/.archive/test/test2.csv").to_path_buf();

        assert_eq!(expected, get_archive_path(&datadir, project));
    }
}
