//! Read-only command-line interface over the same engine the app uses.
use cym_core::{model::*, policy, ports::ScanControl, Engine};
use std::process::exit;

const HELP: &str = "CleanYourMac · independent macOS cleanup inspector

cym modules
cym scan MODULE [ROOT ...] [--json]

Scans are read-only. Reviewed actions are available in the native app.
JSON output omits process commands and abbreviates the home directory.
Exit codes: 0 complete, 1 failed, 2 invalid invocation, 3 partial coverage.";

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    exit(2)
}
fn bytes(value: u64) -> String {
    let units = ["bytes", "KB", "MB", "GB", "TB"];
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1000. && unit < units.len() - 1 {
        size /= 1000.;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} bytes")
    } else {
        format!("{size:.1} {}", units[unit])
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return;
    }
    let engine = Engine::native();
    if args[0] == "modules" {
        for m in engine.registry.descriptors() {
            println!("{}\t{}", m.id, m.name);
        }
        return;
    }
    if args[0] != "scan" || args.len() < 2 {
        fail("Unknown command. Use cym modules or cym --help.");
    }
    let Some(module) = engine.registry.get(&args[1]) else {
        fail("Unknown module. Use cym modules.");
    };
    let descriptor = module.descriptor();
    let rest = &args[2..];
    if rest.iter().any(|a| a.starts_with("--") && a != "--json") {
        fail("Unknown option. Supported scan option: --json.");
    }
    let json = rest.iter().any(|a| a == "--json");
    let mut roots: Vec<String> = rest
        .iter()
        .filter(|a| *a != "--json")
        .map(|r| policy::canonical(r))
        .collect();
    if roots.is_empty() {
        roots = engine.project_roots();
    }
    if descriptor.uses_roots && roots.is_empty() {
        fail(&format!(
            "Choose at least one folder: cym scan {} /path/to/folder",
            descriptor.id
        ));
    }
    let context = ScanContext {
        roots,
        ignored_process_names: ["ssh-agent", "gpg-agent", "keyboxd", "dirmngr"]
            .map(String::from)
            .into(),
        ..Default::default()
    };
    let mut report = engine.scan_report(
        std::slice::from_ref(&descriptor.id),
        &context,
        &ScanControl::default(),
    );
    for finding in &mut report.findings {
        finding.details.retain(|d| d.label != "Command");
    }
    let home = policy::home();
    if json {
        let output = serde_json::json!({
            "findings": report.findings,
            "warnings": report.warnings,
            "complete": report.warnings.is_empty(),
        });
        match serde_json::to_string_pretty(&output) {
            Ok(text) => println!(
                "{}",
                if home.is_empty() {
                    text
                } else {
                    text.replace(&home, "~")
                }
            ),
            Err(e) => {
                eprintln!("Scan failed: {e}");
                exit(1)
            }
        }
    } else {
        for f in &report.findings {
            let size = f.bytes.map(bytes).unwrap_or_else(|| "—".into());
            let subtitle = if home.is_empty() {
                f.subtitle.clone()
            } else {
                f.subtitle.replace(&home, "~")
            };
            println!("{size}\t{}\t{subtitle}", f.title);
        }
        for w in &report.warnings {
            eprintln!("Warning: {w}");
        }
        println!(
            "{} findings. {}",
            report.findings.len(),
            if report.warnings.is_empty() {
                "Scan complete."
            } else {
                "Partial coverage."
            }
        );
    }
    if !report.warnings.is_empty() {
        exit(3);
    }
}
