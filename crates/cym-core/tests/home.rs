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

#[test]
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
    // One level further down, items are ordinary storage again.
    let caches = scan(&engine, "storage", vec![f.at("home/Library/Caches")]);
    assert_eq!(caches[0].actions, vec![ActionKind::Trash]);

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
