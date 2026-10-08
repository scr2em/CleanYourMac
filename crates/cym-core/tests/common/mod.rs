#![allow(dead_code)]
use cym_core::{adapters::journal::MemoryJournal, model::*, ports::*, Services};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

/// A disposable folder outside `target/` (whose name the duplicate finder ignores).
pub struct Fixture(pub PathBuf);
impl Fixture {
    pub fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.test-fixtures")
            .join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&root).unwrap();
        Self(fs::canonicalize(root).unwrap())
    }
    pub fn path(&self) -> String {
        self.0.to_string_lossy().into()
    }
    pub fn at(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into()
    }
    pub fn write(&self, name: &str, contents: &str) -> String {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path.to_string_lossy().into()
    }
    pub fn dir(&self, name: &str) -> String {
        let path = self.0.join(name);
        fs::create_dir_all(&path).unwrap();
        path.to_string_lossy().into()
    }
    pub fn context(&self) -> ScanContext {
        ScanContext {
            roots: vec![self.path()],
            ..Default::default()
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Moves items into a fixture folder so tests never touch the user's Trash.
pub struct FixtureTrash(pub String);
impl Trash for FixtureTrash {
    fn move_to_trash(&self, path: &str) -> Result<String> {
        fs::create_dir_all(&self.0).map_err(|e| e.to_string())?;
        let target = format!("{}/{}", self.0, uuid::Uuid::new_v4());
        fs::rename(path, &target).map_err(|e| e.to_string())?;
        Ok(target)
    }
}

/// Returns recorded responses in order and records every invocation.
#[derive(Default)]
pub struct StubRunner {
    pub responses: Mutex<Vec<Output>>,
    pub calls: Mutex<Vec<Vec<String>>>,
}
impl StubRunner {
    pub fn new(responses: Vec<Output>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into_iter().rev().collect()),
            calls: Mutex::default(),
        })
    }
    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}
impl CommandRunner for StubRunner {
    fn run(
        &self,
        executable: &str,
        args: &[String],
        _: Duration,
        _: &ScanControl,
    ) -> Result<Output> {
        let mut call = vec![executable.to_owned()];
        call.extend(args.iter().cloned());
        self.calls.lock().unwrap().push(call);
        self.responses
            .lock()
            .unwrap()
            .pop()
            .ok_or_else(|| "Unexpected command".into())
    }
}
pub fn output(text: &str, status: i32) -> Output {
    Output {
        data: text.as_bytes().to_vec(),
        error: if status == 0 {
            String::new()
        } else {
            "fixture failure".into()
        },
        status,
    }
}

#[derive(Default)]
pub struct FakeApps(pub Vec<RunningApp>);
impl Applications for FakeApps {
    fn running(&self) -> Result<Vec<RunningApp>> {
        Ok(self.0.clone())
    }
    fn bundle_info(&self, _: &str) -> BundleInfo {
        BundleInfo::default()
    }
}

/// A process table that tests control completely.
#[derive(Default)]
pub struct FakeProcesses {
    pub rows: Mutex<Vec<Snapshot>>,
    pub signals: Mutex<Vec<(i32, bool)>>,
}
impl ProcessInspector for FakeProcesses {
    fn pids(&self) -> Vec<i32> {
        self.rows
            .lock()
            .unwrap()
            .iter()
            .map(|p| p.identity.pid)
            .collect()
    }
    fn inspect(&self, pid: i32) -> Option<Snapshot> {
        self.rows
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.identity.pid == pid)
            .cloned()
    }
    fn signal(&self, pid: i32, force: bool) -> Result<()> {
        self.signals.lock().unwrap().push((pid, force));
        self.rows.lock().unwrap().retain(|p| p.identity.pid != pid);
        Ok(())
    }
    fn current_uid(&self) -> u32 {
        501
    }
    fn current_pid(&self) -> i32 {
        99
    }
}
pub fn process(path: &str, parent: i32, uid: u32) -> Snapshot {
    Snapshot {
        identity: ProcessIdentity {
            pid: 42,
            uid,
            started_seconds: 100,
            started_microseconds: 2,
            executable: path.into(),
        },
        parent,
        name: Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into(),
        command: "fixture".into(),
        cwd: None,
        cpu_nanos: Some(0),
        memory: Some(0),
    }
}

/// The built-in modules, with every module that reads fixed home-folder locations (shared
/// package stores, caches, toolchains) pointed at `fixture/home`. Tests never see, and so can
/// never act on, the real home folder.
pub fn builtin(fixture: &Fixture) -> cym_core::modules::Registry {
    use cym_core::modules::{
        ai, containers, developer, installers, projects, system, toolchains, xcode,
    };
    let home = Some(fixture.at("home"));
    cym_core::modules::builtin()
        .register(Arc::new(developer::DependenciesModule {
            home: home.clone(),
            ..Default::default()
        }))
        .register(Arc::new(developer::ArtifactsModule { home: home.clone() }))
        .register(Arc::new(developer::CachesModule {
            home: home.clone(),
            ..Default::default()
        }))
        .register(Arc::new(toolchains::ToolchainsModule {
            home: home.clone(),
            ..Default::default()
        }))
        .register(Arc::new(xcode::XcodeModule {
            home: home.clone(),
            developer_dir: Some(fixture.at("home/Applications/Xcode.app/Contents/Developer")),
        }))
        .register(Arc::new(containers::ContainersModule {
            home: home.clone(),
        }))
        .register(Arc::new(projects::ProjectsModule {
            home: home.clone(),
            ..Default::default()
        }))
        .register(Arc::new(installers::InstallersModule {
            home: home.clone(),
        }))
        // System folders are measured only by the tests that point them at fixtures.
        .register(Arc::new(system::SystemModule {
            folders: Some(vec![]),
            time_machine: false,
        }))
        .register(Arc::new(ai::AiToolsModule {
            home,
            env: Some(vec![]),
            ..Default::default()
        }))
}

/// Native filesystem adapters with fixture Trash and an in-memory journal.
pub fn services(fixture: &Fixture) -> Services {
    Services {
        trash: Arc::new(FixtureTrash(fixture.at(".Trash"))),
        apps: Arc::new(FakeApps::default()),
        processes: Arc::new(FakeProcesses::default()),
        ..Services::with_journal(Arc::new(MemoryJournal::default()))
    }
}
pub fn file_finding(id: &str, module: &str, identity: FileIdentity) -> Finding {
    let mut f = Finding::new(module, id, id, Resource::File { file: identity }, "fixture");
    f.id = id.into();
    f.actions = vec![ActionKind::Trash];
    f
}
