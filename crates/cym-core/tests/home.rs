//! Tools that read the current user's home folder directly. This file is its own test
//! process, so pointing `HOME` at a fixture cannot affect other tests.
mod common;
use common::*;
use cym_core::{model::*, ports::*, Engine};

fn scan(engine: &Engine, module: &str, roots: Vec<String>) -> Vec<Finding> {
    let context = ScanContext {
        roots,
        ..Default::default()
    };
    engine
        .scan_report(&[module.into()], &context, &ScanControl::default())
        .findings
}

/// Each case sets `HOME`, so they run one after another in a single test.
#[test]
fn tools_reading_the_home_folder() {
    library_folders_and_finder_files_are_left_alone();
    tool_homes_are_left_to_their_tools();
}

fn library_folders_and_finder_files_are_left_alone() {
    let f = Fixture::new();
    let home = f.dir("home");
    std::env::set_var("HOME", &home);
    f.write("home/Library/Application Support/app/data", "x");
    f.write("home/Library/Caches/com.example.app/blob", "x");
    for folder in ["Downloads", ".Trash"] {
        f.write(&format!("home/{folder}/.DS_Store"), "x");
        f.write(&format!("home/{folder}/.localized"), "");
        f.write(&format!("home/{folder}/report.pdf"), "x");
    }
    let engine = Engine::new(services(&f), builtin(&f));

    // Storage Explorer inside the Library: its own folders are shown but never offered.
    let library = scan(&engine, "storage", vec![f.at("home/Library")]);
    let support = library
        .iter()
        .find(|r| r.title == "Application Support")
        .expect("listed");
    assert!(support.actions.is_empty());
    assert!(support
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("macOS and app data"));
    // Further down, an app's data is listed for its size but never offered, as in Large
    // Files: it is removed in its app, or with the tool that lists it and checks it first.
    let caches = scan(&engine, "storage", vec![f.at("home/Library/Caches")]);
    assert!(caches[0].actions.is_empty());
    assert!(caches[0]
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("app's or tool's data"));
    // Outside app and tool folders, items are ordinary storage.
    f.write("home/Projects/notes.txt", "x");
    let projects = scan(&engine, "storage", vec![f.at("home/Projects")]);
    assert_eq!(projects[0].actions, vec![ActionKind::Trash]);

    // Downloads and Trash never list Finder's own files.
    for module in ["downloads", "trash"] {
        let names: Vec<String> = scan(&engine, module, vec![home.clone()])
            .into_iter()
            .map(|r| r.title)
            .collect();
        assert_eq!(names, vec!["report.pdf".to_owned()], "{module}");
    }
    let top = scan(&engine, "storage", vec![f.at("home/Downloads")]);
    assert!(top
        .iter()
        .all(|r| r.title != ".localized" && r.title != ".DS_Store"));
}

/// Conda installs and virtual machines in the home folder belong to their tools: Large
/// Files lists their files blocked, Build Artifacts and Exact Duplicates stay out.
fn tool_homes_are_left_to_their_tools() {
    let f = Fixture::new();
    let home = f.dir("home");
    std::env::set_var("HOME", &home);
    let big = |path: &str| {
        let path = f.write(path, "");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(150_000_000)
            .unwrap();
    };
    big("home/VirtualBox VMs/dev/dev.vdi");
    f.write("home/miniconda3/lib/python3.12/site.py", "");
    f.write("home/miniconda3/lib/python3.12/__pycache__/site.pyc", "");
    let body = "equal contents ".repeat(400);
    f.write("home/miniconda3/pkgs/a/info.txt", &body);
    f.write("home/miniconda3/envs/b/info.txt", &body);
    let engine = Engine::new(services(&f), builtin(&f));
    let large = scan(&engine, "large", vec![home.clone()]);
    let vdi = large
        .iter()
        .find(|r| r.title == "dev.vdi")
        .unwrap_or_else(|| panic!("{large:?}"));
    assert!(vdi.actions.is_empty());
    assert!(vdi.blocked_reason.is_some());
    assert!(scan(&engine, "artifacts", vec![home.clone()]).is_empty());
    assert!(scan(&engine, "duplicates", vec![home]).is_empty());
}
