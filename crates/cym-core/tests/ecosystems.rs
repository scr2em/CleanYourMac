mod common;
use common::*;
use cym_core::{
    model::*,
    modules::{
        self,
        developer::{ArtifactsModule, CachesModule, DependenciesModule},
        toolchains::ToolchainsModule,
        ScanModule,
    },
    ports::*,
    Engine, Services,
};
use std::{fs, os::unix::fs::symlink, sync::Arc};

/// The ecosystem of a build artifact, or of installed dependencies.
fn ecosystem(f: &Fixture, module: &ArtifactsModule, path: &str) -> Option<&'static str> {
    let (s, path) = (services(f), f.at(path));
    let dependencies = DependenciesModule::default();
    module
        .matches(&s, &path)
        .or_else(|| dependencies.matches(&s, &path))
        .map(|m| m.rule.ecosystem)
}

#[test]
fn artifact_rules_need_evidence_from_their_ecosystem() {
    let f = Fixture::new();
    let module = ArtifactsModule::default();
    let cases: &[(&[&str], &str, Option<&str>)] = &[
        (
            &["flutter/pubspec.yaml"],
            "flutter/.dart_tool",
            Some("Flutter / Dart"),
        ),
        (
            &["flutter/pubspec.yaml"],
            "flutter/build",
            Some("Flutter / Dart"),
        ),
        (&["ng/angular.json"], "ng/.angular", Some("Angular")),
        (&["ng2/package.json"], "ng2/.angular", None),
        (
            &["kit/svelte.config.js"],
            "kit/.svelte-kit",
            Some("SvelteKit"),
        ),
        (&["nx/nx.json"], "nx/.nx/cache", Some("Nx")),
        (&["zig/build.zig"], "zig/.zig-cache", Some("Zig")),
        // A CMake build tree needs its cache inside, since build/ is often source.
        (&["cpp0/CMakeLists.txt"], "cpp0/build", None),
        (
            &[
                "cpp/CMakeLists.txt",
                "cpp/cmake-build-debug/CMakeCache.txt",
                "cpp/cmake-build-debug/CMakeFiles/x",
            ],
            "cpp/cmake-build-debug",
            Some("CMake"),
        ),
        (&["next/next.config.mjs"], "next/.next", Some("Next.js")),
        (&["turbo/turbo.json"], "turbo/.turbo", Some("Turborepo")),
        (&["js/package.json"], "js/.next", None),
        (
            &["berry/package.json", "berry/.yarnrc.yml"],
            "berry/.yarn/unplugged",
            Some("Yarn"),
        ),
        // The Yarn cache can be the committed dependency store.
        (&[], "berry/.yarn/cache", None),
        (&["mvn/pom.xml"], "mvn/target", Some("Maven")),
        (&["kt/settings.gradle.kts"], "kt/.gradle", Some("Gradle")),
        (&["ex0/mix.exs"], "ex0/deps", None),
        (&["ex/mix.exs", "ex/mix.lock"], "ex/deps", Some("Elixir")),
        (
            &["hs/app.cabal"],
            "hs/dist-newstyle",
            Some("Haskell (Cabal)"),
        ),
        (&["elm/elm.json"], "elm/elm-stuff", Some("Elm")),
        (&["py/setup.py"], "py/src/pkg.egg-info", Some("Python")),
        (&["py2/mod.py"], "py2/__pycache__", Some("Python")),
        (&["py3/README.md"], "py3/__pycache__", None),
        (&["tf/main.tf"], "tf/.terraform", Some("Terraform")),
        (
            &["unreal/Game.uproject"],
            "unreal/Intermediate",
            Some("Unreal Engine"),
        ),
        // Unity generates .csproj files, but its rule comes first.
        (
            &[
                "unity/ProjectSettings/ProjectVersion.txt",
                "unity/Assets/Scene.unity",
                "unity/Game.csproj",
            ],
            "unity/obj",
            Some("Unity"),
        ),
        (&["misc/README.md"], "misc/Library", None),
    ];
    for (files, artifact, expected) in cases {
        for file in *files {
            f.write(file, "x");
        }
        f.dir(artifact);
        assert_eq!(ecosystem(&f, &module, artifact), *expected, "{artifact}");
    }
}

#[test]
fn dotnet_and_xcode_evidence_match_by_extension() {
    let f = Fixture::new();
    let module = ArtifactsModule::default();
    f.write("App/App.csproj", "<Project/>");
    f.dir("App/bin/Debug");
    f.dir("App/obj");
    f.write("Scripts/README.md", "x");
    f.dir("Scripts/bin");
    f.write("Solution/All.sln", "x");
    f.dir("Solution/bin");
    // A file named like the pattern is not a project folder.
    f.write("Odd/Fake.xcodeproj", "x");
    f.dir("Odd/DerivedData");
    f.dir("Mac/App.xcodeproj");
    f.dir("Mac/DerivedData");
    assert_eq!(ecosystem(&f, &module, "App/bin"), Some(".NET"));
    let m = module.matches(&services(&f), &f.at("App/obj")).unwrap();
    assert_eq!(m.evidence, "App.csproj");
    assert_eq!(ecosystem(&f, &module, "Scripts/bin"), None);
    assert_eq!(ecosystem(&f, &module, "Solution/bin"), Some(".NET"));
    assert_eq!(ecosystem(&f, &module, "Odd/DerivedData"), None);
    assert_eq!(ecosystem(&f, &module, "Mac/DerivedData"), Some("Xcode"));
}

#[test]
fn react_native_layouts_use_the_project_root_as_evidence() {
    let f = Fixture::new();
    let module = ArtifactsModule::default();
    let s = services(&f);
    f.write("rn/package.json", "{}");
    f.write("rn/app.json", "{}");
    f.write("rn/ios/Podfile", "x");
    f.write("rn/ios/Podfile.lock", "x");
    f.dir("rn/ios/Pods");
    f.write("rn/android/app/build.gradle", "x");
    f.dir("rn/android/app/build");
    f.dir("rn/android/lib/build");
    let dependencies = DependenciesModule::default();
    assert_eq!(module.matches(&s, &f.at("rn/ios/Pods")), None);
    let pods = dependencies.matches(&s, &f.at("rn/ios/Pods")).unwrap();
    assert_eq!(pods.rule.ecosystem, "React Native");
    assert_eq!(pods.project, f.at("rn"));
    assert_eq!(pods.relative, "ios/Pods");
    assert_eq!(
        ecosystem(&f, &module, "rn/android/app/build"),
        Some("React Native")
    );
    // A nested build folder needs its own Gradle file beside it.
    assert_eq!(ecosystem(&f, &module, "rn/android/lib/build"), None);
    // Without the app manifest, Pods are still identified by the Podfile beside them.
    fs::remove_file(f.at("rn/app.json")).unwrap();
    let pods = dependencies.matches(&s, &f.at("rn/ios/Pods")).unwrap();
    assert_eq!(pods.rule.ecosystem, "CocoaPods");
    assert_eq!(pods.project, f.at("rn/ios"));
    // Pods without a lockfile beside them are never offered.
    fs::remove_file(f.at("rn/ios/Podfile.lock")).unwrap();
    assert_eq!(ecosystem(&f, &module, "rn/ios/Pods"), None);
    // A build folder in an unrelated ios folder has no evidence.
    f.dir("other/ios/build");
    assert_eq!(ecosystem(&f, &module, "other/ios/build"), None);
}

#[test]
fn virtual_environments_need_pyvenv_cfg_and_a_project() {
    let f = Fixture::new();
    let module = ArtifactsModule::default();
    f.write("a/pyproject.toml", "x");
    f.write("a/.venv/pyvenv.cfg", "home = /usr/bin");
    f.write("b/pyproject.toml", "x");
    f.dir("b/venv");
    f.write("c/.venv/pyvenv.cfg", "home = /usr/bin");
    assert_eq!(ecosystem(&f, &module, "a/.venv"), Some("Python"));
    assert_eq!(ecosystem(&f, &module, "b/venv"), None);
    assert_eq!(ecosystem(&f, &module, "c/.venv"), None);
}

#[test]
fn artifact_scan_reports_ecosystems_and_skips_dependency_stores() {
    let f = Fixture::new();
    f.write("web/package.json", "{}");
    f.write("web/app.json", "{}");
    f.write("web/ios/build/a.o", "x");
    f.dir("web/ios/App.xcodeproj");
    f.write("web/node_modules/dep/Cargo.toml", "x");
    f.write("web/node_modules/dep/target/x", "x");
    f.write("api/pyproject.toml", "x");
    f.write("api/.venv/pyvenv.cfg", "x");
    f.write("api/.venv/lib/site-packages/m.py", "x");
    f.write("api/.venv/lib/site-packages/__pycache__/m.pyc", "x");
    f.write("game/Game.uproject", "{}");
    f.write("game/Saved/Autosaves/a.umap", "x");
    let report = Engine::new(services(&f), builtin(&f)).scan_report(
        &["artifacts".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut titles: Vec<_> = report.findings.iter().map(|f| f.title.as_str()).collect();
    titles.sort();
    assert_eq!(titles, ["Saved", "ios/build"], "{:?}", report.warnings);
    let ios = report
        .findings
        .iter()
        .find(|f| f.title == "ios/build")
        .unwrap();
    assert_eq!(ios.value("Ecosystem"), Some("React Native"));
    assert_eq!(ios.value("Project"), Some(f.at("web").as_str()));
    assert_eq!(ios.value("Evidence"), Some("app.json"));
    assert!(ios.reason.contains("next iOS build"));
    assert_eq!(ios.risk, Risk::Rebuild);
    assert_eq!(ios.actions, vec![ActionKind::Trash]);
    let saved = report.findings.iter().find(|f| f.title == "Saved").unwrap();
    assert_eq!(saved.risk, Risk::Review);
    assert!(saved.reason.contains("autosaves"));
    // Every finding carries its ecosystem's logo.
    let brand = |title: &str| {
        let f = report.findings.iter().find(|f| f.title == title).unwrap();
        f.brand.as_deref()
    };
    assert_eq!(brand("ios/build"), Some("react"));
    assert_eq!(brand("Saved"), Some("unrealengine"));
}

#[test]
fn dependencies_cover_every_ecosystem_once_and_without_descending() {
    let f = Fixture::new();
    f.write("app/package.json", "{}");
    f.write("app/yarn.lock", "x");
    f.write("app/node_modules/dep/index.js", "x");
    f.write("app/pyproject.toml", "x");
    f.write("app/.venv/pyvenv.cfg", "home = /usr/bin");
    // Nothing inside an installed environment is listed separately.
    f.write("app/.venv/lib/tool/package.json", "{}");
    f.write("app/.venv/lib/tool/node_modules/x/index.js", "x");
    f.write("app/ios/Podfile", "x");
    f.write("app/ios/Podfile.lock", "x");
    f.write("app/ios/Pods/Lib/a.m", "x");
    f.write("site/composer.json", "{}");
    f.write("site/composer.lock", "{}");
    f.write("site/vendor/autoload.php", "<?php");
    f.write("site/vendor/pkg/composer.json", "{}");
    f.write("site/vendor/pkg/composer.lock", "{}");
    f.write("site/vendor/pkg/vendor/autoload.php", "<?php");
    f.write("gosvc/go.mod", "module example.com/svc");
    f.write("gosvc/vendor/modules.txt", "# example.com/dep v1.0.0");
    f.write("crate/Cargo.toml", "[package]");
    f.write("crate/Cargo.lock", "x");
    f.write("crate/.cargo/config.toml", "[source.vendored-sources]");
    f.write("crate/vendor/dep/.cargo-checksum.json", "{}");
    f.write("kit/Package.swift", "// swift-tools-version:5.9");
    f.write("kit/.build/checkouts/Dep/Package.swift", "x");
    f.write("kit/.build/arm64-apple-macosx/debug/Kit.o", "x");
    f.write("edge/deno.json", "{}");
    f.write("edge/deno.lock", "{}");
    f.write("edge/node_modules/.deno/x/index.js", "x");
    // A folder named like a dependency store without its project's evidence is left alone.
    f.write("notes/vendor/autoload.php", "<?php");
    f.write("plain/vendor/modules.txt", "x");
    let dependencies = modules::developer::DependenciesModule {
        home: Some(f.at("home")),
        ..Default::default()
    };
    let engine = Engine::new(services(&f), builtin(&f).register(Arc::new(dependencies)));
    let report = engine.scan_report(&["node".into()], &f.context(), &ScanControl::default());
    let mut found: Vec<_> = report
        .findings
        .iter()
        .map(|r| {
            (
                r.title.as_str(),
                r.value("Artifact").unwrap(),
                r.brand.as_deref().unwrap_or_default(),
            )
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("app", ".venv", "python"),
            ("app", "node_modules", "yarn"),
            ("crate", "vendor", "rust"),
            ("edge", "node_modules", "deno"),
            ("gosvc", "vendor", "go"),
            ("ios", "Pods", "cocoapods"),
            ("kit", ".build/checkouts", "swift"),
            ("site", "vendor", "composer"),
        ],
        "{:?}",
        report.warnings
    );
    assert!(report.findings.iter().all(|r| r.risk == Risk::Rebuild
        && r.actions == vec![ActionKind::Trash]
        && r.blocked_reason.is_none()));
    let pods = report.findings.iter().find(|r| r.title == "ios").unwrap();
    assert_eq!(pods.value("Official command"), Some("pod install"));
    // Build Artifacts neither lists nor searches inside installed dependencies.
    f.write("app/.venv/lib/__pycache__/m.pyc", "x");
    f.write("app/.venv/lib/m.py", "x");
    // Build Artifacts lists SwiftPM's build output without the fetched packages beside it.
    let artifacts =
        engine.scan_report(&["artifacts".into()], &f.context(), &ScanControl::default());
    let titles: Vec<_> = artifacts
        .findings
        .iter()
        .map(|r| r.title.as_str())
        .collect();
    assert_eq!(
        titles,
        [".build/arm64-apple-macosx"],
        "{:?}",
        artifacts.warnings
    );
}

#[test]
fn dependencies_list_shared_package_stores_and_never_search_them() {
    let f = Fixture::new();
    f.write("home/.cargo/registry/src/index/serde-1.0/Cargo.toml", "x");
    f.write(
        "home/.cargo/registry/src/index/serde-1.0/vendor/modules.txt",
        "x",
    );
    f.write("home/.m2/repository/org/x.jar", "x");
    f.write("home/go/pkg/mod/example.com/m@v1/go.mod", "module m");
    f.write("home/go/pkg/mod/example.com/m@v1/vendor/modules.txt", "x");
    f.write("home/.npm/_cacache/index", "x");
    let dependencies = modules::developer::DependenciesModule {
        home: Some(f.at("home")),
        ..Default::default()
    };
    let engine = Engine::new(services(&f), builtin(&f).register(Arc::new(dependencies)));
    let report = engine.scan_report(&["node".into()], &f.context(), &ScanControl::default());
    let mut found: Vec<_> = report
        .findings
        .iter()
        .map(|r| (r.title.as_str(), r.value("Scope"), r.actions.is_empty()))
        .collect();
    found.sort();
    let shared = Some("Shared by all projects");
    assert_eq!(
        found,
        [
            ("Cargo registry sources", shared, false),
            // Go makes its module cache read-only; only `go clean -modcache` removes it.
            ("Go module cache", shared, true),
            ("Maven repository", shared, false),
            ("npm cache", shared, false),
        ],
        "{:?}",
        report.warnings
    );
    let npm = report
        .findings
        .iter()
        .find(|r| r.title == "npm cache")
        .unwrap();
    assert_eq!(npm.brand.as_deref(), Some("npm"));
    assert!(npm.reason.contains("npm cache clean --force"));
}

#[test]
fn committed_vendor_folders_are_left_out() {
    let f = Fixture::new();
    let s = services(&f);
    let k = ScanControl::default();
    let git = cym_core::git::Git(s.commands.as_ref());
    if git
        .run(&f.path(), &["--version"], &k)
        .map(|o| o.status)
        .ok()
        != Some(0)
    {
        return;
    }
    f.write("svc/go.mod", "module example.com/svc");
    f.write("svc/vendor/modules.txt", "# example.com/dep v1.0.0");
    let scan = || {
        let dependencies = modules::developer::DependenciesModule {
            home: Some(f.at("home")),
            ..Default::default()
        };
        Engine::new(services(&f), builtin(&f).register(Arc::new(dependencies)))
            .scan_report(&["node".into()], &f.context(), &ScanControl::default())
            .findings
            .len()
    };
    assert_eq!(scan(), 1);
    for args in [&["init", "-q"][..], &["add", "vendor/modules.txt"]] {
        assert_eq!(git.run(&f.at("svc"), args, &k).unwrap().status, 0);
    }
    assert_eq!(scan(), 0);
}

#[test]
fn brands_follow_ecosystems_and_every_brand_has_a_logo() {
    use cym_core::brand;
    for (ecosystem, slug) in [
        ("Next.js", "nextdotjs"),
        ("Flutter / Dart", "flutter"),
        ("Rust (Cargo)", "rust"),
        ("PHP (Composer)", "composer"),
        ("Haskell (Stack)", "haskell"),
        ("Unreal Engine", "unrealengine"),
        ("Node.js", "nodedotjs"),
        ("pnpm", "pnpm"),
        ("java", "openjdk"),
    ] {
        assert_eq!(brand::for_ecosystem(ecosystem), Some(slug), "{ecosystem}");
    }
    assert_eq!(brand::for_ecosystem("Something new"), None);
    // The app's logo folder must hold a coloured SVG for every slug the core can return.
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../Sources/CleanYourMacDesignSystem/BrandIcons");
    for slug in brand::slugs() {
        let svg = fs::read_to_string(folder.join(format!("{slug}.svg")))
            .unwrap_or_else(|_| panic!("no logo for {slug}"));
        assert!(
            svg.starts_with("<svg fill=\"#"),
            "{slug} has no brand colour"
        );
    }
}

#[test]
fn artifact_actions_recheck_project_evidence() {
    let f = Fixture::new();
    f.write("crate/Cargo.toml", "[package]");
    f.write("crate/target/debug/app", "x");
    f.write("other/Cargo.toml", "[package]");
    f.write("other/target/debug/app", "x");
    let s = Services {
        commands: StubRunner::new(vec![output("", 0)]),
        ..services(&f)
    };
    let engine = Engine::new(s, builtin(&f));
    let report = engine.scan_report(&["artifacts".into()], &f.context(), &ScanControl::default());
    assert_eq!(report.findings.len(), 2);
    assert!(report
        .findings
        .iter()
        .all(|f| f.value("Official command") == Some("cargo clean")));
    fs::remove_file(f.at("other/Cargo.toml")).unwrap();
    let results = engine.execute(
        &ActionRequest {
            findings: report.findings.clone(),
            kind: ActionKind::Trash,
            context: f.context(),
            acknowledged: vec![],
            force: false,
        },
        &ScanControl::default(),
    );
    let outcome = |path: &str| {
        let id = report
            .findings
            .iter()
            .find(|x| x.resource.path() == Some(f.at(path).as_str()))
            .unwrap()
            .id
            .clone();
        results
            .iter()
            .find(|r| r.finding_id.as_deref() == Some(id.as_str()))
            .unwrap()
            .outcome
    };
    assert_eq!(outcome("crate/target"), Outcome::Applied);
    assert_eq!(outcome("other/target"), Outcome::Failed);
    assert!(fs::metadata(f.at("other/target")).is_ok());
    assert!(fs::metadata(f.at("crate/target")).is_err());
}

#[test]
fn caches_report_known_locations_with_an_ecosystem() {
    let f = Fixture::new();
    let home = f.dir("home");
    f.write("home/.gradle/caches/modules-2/a.jar", "x");
    f.write("home/Library/Caches/ms-playwright/chromium/a", "x");
    f.write("home/.cache/huggingface/hub/models--a/blob", "x");
    f.write("home/.cache/huggingface/token", "secret");
    f.write("home/.pub-cache/hosted/pub.dev/a/pubspec.yaml", "x");
    f.write("home/.pub-cache/bin/tool", "x");
    let module = CachesModule {
        home: Some(home.clone()),
        ..Default::default()
    };
    let roots = module.action_roots();
    for location in &module.locations {
        let fixed: Vec<_> = location
            .path
            .split('/')
            .take_while(|p| !p.contains('*'))
            .collect();
        assert!(roots.contains(&format!("{home}/{}", fixed.join("/"))));
    }
    let registry = builtin(&f).register(Arc::new(module));
    let report = Engine::new(services(&f), registry).scan_report(
        &["caches".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut rows: Vec<_> = report
        .findings
        .iter()
        .map(|f| (f.title.as_str(), f.value("Ecosystem").unwrap_or("")))
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        [
            ("Command-line tool caches (~/.cache)", "Command-line tools"),
            ("Gradle caches", "Gradle"),
            ("Playwright browsers", "JavaScript"),
            ("Pub hosted packages", "Flutter / Dart"),
        ],
        "{:?}",
        report.warnings
    );
    assert!(report
        .findings
        .iter()
        .all(|f| f.actions == [ActionKind::Trash]));
    // ~/.cache is one reviewable item that names what is inside and the login it holds.
    let tool_cache = report
        .findings
        .iter()
        .find(|f| f.title.starts_with("Command-line tool caches"))
        .unwrap();
    assert_eq!(tool_cache.risk, Risk::Review);
    assert_eq!(tool_cache.value("Contains"), Some("huggingface"));
    assert_eq!(tool_cache.value("Sign-in"), Some("Hugging Face token"));
    assert!(tool_cache.reason.contains("sign in again"));
}

#[test]
fn caches_block_go_modules_expand_ide_versions_and_wait_for_owning_apps() {
    let f = Fixture::new();
    let home = f.dir("home");
    f.write("home/go/pkg/mod/cache/x", "x");
    f.write(
        "home/Library/Caches/JetBrains/IntelliJIdea2024.1/caches/x",
        "x",
    );
    f.write("home/Library/Caches/JetBrains/PyCharm2024.2/caches/x", "x");
    f.write("home/Library/Application Support/Code/CachedData/x", "x");
    f.write(
        "home/Library/Application Support/Code/User/settings.json",
        "{}",
    );
    let module = Arc::new(CachesModule {
        home: Some(home),
        ..Default::default()
    });
    let s = services(&f);
    let report = Engine::new(s.clone(), builtin(&f).register(module.clone())).scan_report(
        &["caches".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut titles: Vec<_> = report.findings.iter().map(|f| f.title.as_str()).collect();
    titles.sort();
    assert_eq!(
        titles,
        [
            "Go module cache",
            "JetBrains IDE caches · IntelliJIdea2024.1",
            "JetBrains IDE caches · PyCharm2024.2",
            "VS Code cached data",
        ]
    );
    let go = report
        .findings
        .iter()
        .find(|f| f.title == "Go module cache")
        .unwrap();
    assert!(go.actions.is_empty());
    assert!(go
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("go clean -modcache"));
    let vscode = report
        .findings
        .iter()
        .find(|f| f.title == "VS Code cached data")
        .unwrap();
    let k = ScanControl::default();
    assert!(module.preflight(&s, vscode, ActionKind::Trash, &k).is_ok());
    assert!(module.in_use(&s, vscode, ActionKind::Trash).is_none());
    let running = Services {
        apps: Arc::new(FakeApps(vec![RunningApp {
            pid: 7,
            bundle_id: "com.microsoft.VSCode".into(),
            path: "/Applications/Visual Studio Code.app".into(),
        }])),
        ..s
    };
    // A running owner is an overridable "in use" reason, not a hard refusal.
    assert!(module
        .preflight(&running, vscode, ActionKind::Trash, &k)
        .is_ok());
    let error = module.in_use(&running, vscode, ActionKind::Trash).unwrap();
    assert!(error.contains("Visual Studio Code"), "{error}");
}

/// A home folder with several version managers, each with a default and a stale version.
fn toolchain_home(f: &Fixture) -> String {
    let home = f.dir("home");
    f.write(
        "home/.rustup/settings.toml",
        "default_toolchain = \"stable-aarch64-apple-darwin\"\nprofile = \"default\"\n\n[overrides]\n\"/work/old\" = \"nightly-2024-01-01-aarch64-apple-darwin\"\n",
    );
    for t in [
        "stable-aarch64-apple-darwin",
        "nightly-2024-01-01-aarch64-apple-darwin",
        "1.70.0-aarch64-apple-darwin",
    ] {
        f.write(&format!("home/.rustup/toolchains/{t}/bin/rustc"), "x");
    }
    f.write("home/.nvm/alias/default", "lts/iron\n");
    f.write("home/.nvm/alias/lts/iron", "v20.11.0\n");
    f.write("home/.nvm/versions/node/v20.11.0/bin/node", "x");
    f.write("home/.nvm/versions/node/v18.0.0/bin/node", "x");
    f.write("home/.pyenv/version", "3.12.1\n");
    f.write("home/.pyenv/versions/3.12.1/bin/python", "x");
    f.write("home/.pyenv/versions/3.11.0/bin/python", "x");
    f.write("home/fvm/versions/3.22.0/bin/flutter", "x");
    f.write("home/fvm/versions/3.19.0/bin/flutter", "x");
    symlink(f.at("home/fvm/versions/3.22.0"), f.at("home/fvm/default")).unwrap();
    f.write("home/.sdkman/candidates/java/21.0.1-tem/bin/java", "x");
    f.write("home/.sdkman/candidates/java/17.0.9-tem/bin/java", "x");
    symlink("21.0.1-tem", f.at("home/.sdkman/candidates/java/current")).unwrap();
    f.write(
        "home/.volta/tools/user/platform.json",
        r#"{"node":{"runtime":"20.1.0","npm":null}}"#,
    );
    f.write("home/.volta/tools/image/node/20.1.0/bin/node", "x");
    f.write("home/.volta/tools/image/node/18.2.0/bin/node", "x");
    f.write("home/.tool-versions", "nodejs 20.0.0 # pinned\n");
    f.write("home/.asdf/installs/nodejs/20.0.0/bin/node", "x");
    f.write("home/.asdf/installs/nodejs/18.0.0/bin/node", "x");
    f.write(
        "home/Library/Android/sdk/system-images/android-34/google_apis/arm64-v8a/system.img",
        "x",
    );
    f.write(
        "home/Library/Android/sdk/system-images/android-30/default/x86_64/system.img",
        "x",
    );
    f.write(
        "home/.android/avd/Pixel.avd/config.ini",
        "hw.lcd.density=440\nimage.sysdir.1=system-images/android-34/google_apis/arm64-v8a/\n",
    );
    f.write("home/.android/avd/Pixel.ini", "path=x");
    f.write("home/.android/avd/Old.avd/config.ini", "x");
    f.write("home/.android/avd/Old.avd/multiinstance.lock", "");
    home
}

#[test]
fn toolchains_list_versions_and_block_the_ones_in_use() {
    let f = Fixture::new();
    let home = toolchain_home(&f);
    let module = ToolchainsModule {
        home: Some(home.clone()),
        ..Default::default()
    };
    assert!(module
        .action_roots()
        .contains(&format!("{home}/.rustup/toolchains")));
    assert!(module
        .action_roots()
        .contains(&format!("{home}/.android/avd")));
    let registry = builtin(&f).register(Arc::new(module));
    let report = Engine::new(services(&f), registry).scan_report(
        &["toolchains".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut rows: Vec<_> = report
        .findings
        .iter()
        .map(|f| {
            (
                f.value("Ecosystem").unwrap_or("").to_owned(),
                f.value("Version").unwrap_or("").to_owned(),
                f.blocked_reason.is_some(),
            )
        })
        .collect();
    rows.sort();
    let expected: Vec<(String, String, bool)> = [
        // Old's lock file is stale: no emulator is running.
        ("Android", "Old", false),
        ("Android", "Pixel", false),
        ("Android", "android-30 · default · x86_64", false),
        ("Android", "android-34 · google_apis · arm64-v8a", true),
        ("Flutter", "3.19.0", false),
        ("Flutter", "3.22.0", true),
        ("Node.js", "18.2.0", false),
        ("Node.js", "20.1.0", true),
        ("Node.js", "v18.0.0", false),
        ("Node.js", "v20.11.0", true),
        ("Python", "3.11.0", false),
        ("Python", "3.12.1", true),
        ("Rust", "1.70.0-aarch64-apple-darwin", false),
        ("Rust", "nightly-2024-01-01-aarch64-apple-darwin", true),
        ("Rust", "stable-aarch64-apple-darwin", true),
        ("java", "17.0.9-tem", false),
        ("java", "21.0.1-tem", true),
        ("nodejs", "18.0.0", false),
        ("nodejs", "20.0.0", true),
    ]
    .iter()
    .map(|(e, v, b)| (e.to_string(), v.to_string(), *b))
    .collect();
    assert_eq!(rows, expected, "{:?}", report.warnings);
    for f in &report.findings {
        assert_eq!(
            f.actions.is_empty(),
            f.blocked_reason.is_some(),
            "{}",
            f.title
        );
    }
    let pixel = report
        .findings
        .iter()
        .find(|f| f.title == "Android virtual device Pixel")
        .unwrap();
    assert_eq!(pixel.risk, Risk::Review);
    let image = report
        .findings
        .iter()
        .find(|f| f.value("Version") == Some("android-34 · google_apis · arm64-v8a"))
        .unwrap();
    assert!(image.blocked_reason.as_deref().unwrap().contains("Pixel"));
    let java = report
        .findings
        .iter()
        .find(|f| f.title == "java 17.0.9-tem")
        .unwrap();
    assert_eq!(java.value("Manager"), Some("SDKMAN"));
    assert_eq!(java.risk, Risk::Rebuild);
    assert_eq!(
        java.value("Official command"),
        Some("sdk uninstall java 17.0.9-tem")
    );
    let old_image = report
        .findings
        .iter()
        .find(|f| f.value("Version") == Some("android-30 · default · x86_64"))
        .unwrap();
    assert_eq!(
        old_image.value("Official command"),
        Some("sdkmanager --uninstall \"system-images;android-30;default;x86_64\"")
    );
    // Projects can pin a toolchain where the scan cannot see it.
    let rust = report
        .findings
        .iter()
        .find(|f| f.title == "Rust toolchain 1.70.0-aarch64-apple-darwin")
        .unwrap();
    assert_eq!(rust.risk, Risk::Review);
}

#[test]
fn virtual_devices_are_blocked_while_an_emulator_runs() {
    let f = Fixture::new();
    let home = toolchain_home(&f);
    let processes = Arc::new(FakeProcesses::default());
    processes
        .rows
        .lock()
        .unwrap()
        .push(process("/sdk/emulator/qemu/qemu-system-aarch64", 1, 501));
    let s = Services {
        processes,
        ..services(&f)
    };
    let module = ToolchainsModule {
        home: Some(home),
        ..Default::default()
    };
    let report = Engine::new(s, builtin(&f).register(Arc::new(module))).scan_report(
        &["toolchains".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let pixel = report
        .findings
        .iter()
        .find(|f| f.title == "Android virtual device Pixel")
        .unwrap();
    assert!(pixel.actions.is_empty());
    assert!(pixel
        .blocked_reason
        .as_deref()
        .unwrap()
        .contains("qemu-system-aarch64"));
    // With an emulator running, a device holding lock files is the one it runs.
    let old = report
        .findings
        .iter()
        .find(|f| f.title == "Android virtual device Old")
        .unwrap();
    assert!(old
        .blocked_reason
        .as_deref()
        .unwrap()
        .starts_with("The emulator is running this device"));
}

#[test]
fn toolchain_actions_refuse_running_and_default_versions() {
    let f = Fixture::new();
    let home = toolchain_home(&f);
    let module = Arc::new(ToolchainsModule {
        home: Some(home.clone()),
        ..Default::default()
    });
    let processes = Arc::new(FakeProcesses::default());
    let s = Services {
        processes: processes.clone(),
        ..services(&f)
    };
    let engine = Engine::new(s.clone(), builtin(&f).register(module.clone()));
    let report = engine.scan_report(
        &["toolchains".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let stale = report
        .findings
        .iter()
        .find(|f| f.title == "Rust toolchain 1.70.0-aarch64-apple-darwin")
        .unwrap()
        .clone();
    let rustc = format!("{home}/.rustup/toolchains/1.70.0-aarch64-apple-darwin/bin/rustc");
    processes.rows.lock().unwrap().push(process(&rustc, 1, 501));
    let k = ScanControl::default();
    let error = module
        .preflight(&s, &stale, ActionKind::Trash, &k)
        .unwrap_err();
    assert!(error.contains("rustc"), "{error}");
    processes.rows.lock().unwrap().clear();
    assert!(module.preflight(&s, &stale, ActionKind::Trash, &k).is_ok());
    // Making it the default after the scan blocks the action.
    f.write(
        "home/.rustup/settings.toml",
        "default_toolchain = \"1.70.0-aarch64-apple-darwin\"\n",
    );
    assert!(module.preflight(&s, &stale, ActionKind::Trash, &k).is_err());
    f.write(
        "home/.rustup/settings.toml",
        "default_toolchain = \"stable-aarch64-apple-darwin\"\n",
    );
    let results = engine.execute(
        &ActionRequest {
            findings: vec![stale.clone()],
            kind: ActionKind::Trash,
            context: f.context(),
            acknowledged: vec![],
            force: false,
        },
        &k,
    );
    assert_eq!(
        results[0].outcome,
        Outcome::Applied,
        "{}",
        results[0].message
    );
    assert!(fs::metadata(stale.resource.path().unwrap()).is_err());
}

#[test]
fn caches_include_mac_leftovers_without_repeating_developer_caches() {
    let f = Fixture::new();
    let home = f.dir("home");
    f.write("home/Library/Caches/com.example.Editor/blob", "x");
    f.write("home/Library/Caches/com.apple.Safari/blob", "x");
    // Reported as a developer cache already, so not again as an app cache.
    f.write("home/Library/Caches/pnpm/metadata/x", "x");
    f.write("home/Library/Caches/pypoetry/cache/x", "x");
    f.write(
        "home/Library/Containers/com.example.Notes/Data/Library/Caches/x",
        "x",
    );
    f.write(
        "home/Library/Containers/com.example.Notes/Data/Documents/keep.txt",
        "x",
    );
    f.write(
        "home/Library/Saved Application State/com.example.Editor.savedState/windows.plist",
        "x",
    );
    f.write(
        "home/Library/Containers/com.apple.mail/Data/Library/Mail Downloads/a.pdf",
        "x",
    );
    f.write(
        "home/Library/iTunes/iPhone Software Updates/iPhone.ipsw",
        "x",
    );
    f.write(
        "home/Library/Application Support/MobileSync/Backup/00008030-ABC/Manifest.db",
        "x",
    );
    let module = Arc::new(CachesModule {
        home: Some(home.clone()),
        ..Default::default()
    });
    let s = services(&f);
    let report = Engine::new(s.clone(), builtin(&f).register(module.clone())).scan_report(
        &["caches".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut rows: Vec<_> = report
        .findings
        .iter()
        .map(|f| (f.title.as_str(), f.risk))
        .collect();
    rows.sort_by_key(|(title, _)| title.to_string());
    assert_eq!(
        rows,
        [
            ("App cache · Editor", Risk::Rebuild),
            ("App cache · Safari", Risk::Review),
            ("Device backup · 00008030-ABC", Risk::Review),
            ("Mail downloads", Risk::Review),
            ("Poetry cache", Risk::Rebuild),
            ("Sandboxed app cache · Notes", Risk::Rebuild),
            ("Saved window state · Editor", Risk::Rebuild),
            ("iPhone software updates", Risk::Rebuild),
            ("pnpm cache", Risk::Rebuild),
        ],
        "{:?}",
        report.warnings
    );
    let editor = report
        .findings
        .iter()
        .find(|f| f.title == "App cache · Editor")
        .unwrap();
    assert_eq!(editor.value("App"), Some("com.example.Editor"));
    assert_eq!(editor.value("Ecosystem"), Some("macOS"));
    let k = ScanControl::default();
    assert!(module.preflight(&s, editor, ActionKind::Trash, &k).is_ok());
    assert!(module.in_use(&s, editor, ActionKind::Trash).is_none());
    let running = Services {
        apps: Arc::new(FakeApps(vec![RunningApp {
            pid: 9,
            bundle_id: "com.example.Editor".into(),
            path: "/Applications/Editor.app".into(),
        }])),
        ..s
    };
    // A running owner is an overridable "in use" reason, not a hard refusal.
    assert!(module
        .preflight(&running, editor, ActionKind::Trash, &k)
        .is_ok());
    let error = module.in_use(&running, editor, ActionKind::Trash).unwrap();
    assert!(error.contains("Quit Editor"), "{error}");
}

#[test]
fn caches_list_browser_game_and_media_app_caches_by_profile() {
    let f = Fixture::new();
    let home = f.dir("home");
    let chrome = "home/Library/Application Support/Google/Chrome";
    f.write(&format!("{chrome}/Default/Code Cache/js/a"), "x");
    f.write(&format!("{chrome}/Default/History"), "x");
    f.write(&format!("{chrome}/Default/Local Storage/leveldb/a"), "x");
    f.write(
        &format!("{chrome}/Profile 1/Service Worker/CacheStorage/a/b"),
        "x",
    );
    f.write(
        &format!("{chrome}/OptGuideOnDeviceModel/2025.1/weights.bin"),
        "x",
    );
    f.write(
        "home/Library/Application Support/Microsoft Edge/Default/GPUCache/data_0",
        "x",
    );
    f.write(
        "home/Library/Application Support/Steam/steamapps/shadercache/570/a",
        "x",
    );
    f.write(
        "home/Library/Application Support/Steam/steamapps/common/Game/game.bin",
        "x",
    );
    f.write(
        "home/Library/Application Support/Adobe/Common/Media Cache Files/clip.cfa",
        "x",
    );
    f.write("home/.cache/puppeteer/chrome/mac_arm-131/x", "x");
    f.write("home/.cache/uv/wheels-v5/x", "x");
    f.write("home/.cocoapods/repos/trunk/.git/HEAD", "x");
    f.write("home/.gradle/jdks/temurin-17/bin/java", "x");
    f.write("home/Library/Caches/ccache/0/x", "x");
    f.write(
        "home/.vagrant.d/boxes/hashicorp-VAGRANTSLASH-bionic64/0/x",
        "x",
    );
    let module = Arc::new(CachesModule {
        home: Some(home.clone()),
        ..Default::default()
    });
    let roots = module.action_roots();
    assert!(roots.contains(&format!("{home}/Library/Application Support/Google/Chrome")));
    let s = services(&f);
    let report = Engine::new(s.clone(), builtin(&f).register(module.clone())).scan_report(
        &["caches".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut rows: Vec<_> = report
        .findings
        .iter()
        .map(|f| (f.title.as_str(), f.risk))
        .collect();
    rows.sort_by_key(|(title, _)| title.to_string());
    assert_eq!(
        rows,
        [
            ("Adobe media cache", Risk::Rebuild),
            ("Chrome offline web cache · Profile 1", Risk::Rebuild),
            ("Chrome on-device AI model", Risk::Review),
            ("Chrome script cache · Default", Risk::Rebuild),
            ("CocoaPods spec repo · trunk", Risk::Review),
            ("Command-line tool caches (~/.cache)", Risk::Review),
            ("Edge GPU cache · Default", Risk::Rebuild),
            ("Gradle JDKs", Risk::Rebuild),
            ("Puppeteer browsers", Risk::Rebuild),
            ("Steam shader cache", Risk::Rebuild),
            (
                "Vagrant box · hashicorp-VAGRANTSLASH-bionic64",
                Risk::Review
            ),
            ("ccache", Risk::Rebuild),
            ("uv cache", Risk::Review),
        ],
        "{:?}",
        report.warnings
    );
    // Profile data that is not a cache is never listed.
    assert!(report.findings.iter().all(|f| {
        let path = f.resource.path().unwrap_or_default();
        !path.contains("Local Storage") && !path.ends_with("History") && !path.contains("common")
    }));
    let script = report
        .findings
        .iter()
        .find(|f| f.title == "Chrome script cache · Default")
        .unwrap();
    assert_eq!(script.value("Ecosystem"), Some("Google Chrome"));
    assert_eq!(script.actions, [ActionKind::Trash]);
    let k = ScanControl::default();
    assert!(module.preflight(&s, script, ActionKind::Trash, &k).is_ok());
    // A browser cache waits for the browser, not for unrelated developer tools.
    let node = FakeProcesses::default();
    node.rows
        .lock()
        .unwrap()
        .push(process("/opt/homebrew/bin/node", 1, 501));
    let busy = Services {
        processes: Arc::new(node),
        ..s.clone()
    };
    assert!(module.in_use(&busy, script, ActionKind::Trash).is_none());
    let puppeteer = report
        .findings
        .iter()
        .find(|f| f.title == "Puppeteer browsers")
        .unwrap();
    assert!(module.in_use(&busy, puppeteer, ActionKind::Trash).is_some());
    let running = Services {
        apps: Arc::new(FakeApps(vec![RunningApp {
            pid: 7,
            bundle_id: "com.google.Chrome".into(),
            path: "/Applications/Google Chrome.app".into(),
        }])),
        ..s
    };
    let error = module.in_use(&running, script, ActionKind::Trash).unwrap();
    assert!(error.contains("Google Chrome"), "{error}");
}

#[test]
fn caches_managed_by_their_apps_are_shown_but_never_offered() {
    let f = Fixture::new();
    let home = f.dir("home");
    f.write(
        "home/Library/Caches/CloudKit/com.apple.bird/pending/upload.bin",
        "x",
    );
    f.write(
        "home/Library/Application Support/Google/DriveFS/1234567890/content_cache/a",
        "x",
    );
    f.write("home/Library/Messages/Attachments/0a/00/IMG_0001.HEIC", "x");
    f.write("home/Library/Caches/bazel/_bazel_me/abc/execroot/x", "x");
    let module = Arc::new(CachesModule {
        home: Some(home),
        ..Default::default()
    });
    let s = services(&f);
    let report = Engine::new(s.clone(), builtin(&f).register(module.clone())).scan_report(
        &["caches".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut titles: Vec<_> = report.findings.iter().map(|f| f.title.as_str()).collect();
    titles.sort();
    // The iCloud cache is no longer offered as an ordinary app cache.
    assert_eq!(
        titles,
        [
            "Bazel output",
            "Google Drive offline files and cache",
            "Messages attachments",
            "iCloud Drive sync cache",
        ]
    );
    let k = ScanControl::default();
    for f in &report.findings {
        assert!(f.actions.is_empty(), "{} offers no Trash", f.title);
        assert!(f.blocked_reason.is_some(), "{}", f.title);
        assert!(f.bytes.unwrap() > 0, "{} shows its size", f.title);
        assert!(!f.reason.contains("before moving this cache to Trash"));
        // Even a selection made outside the app is refused.
        assert!(module.preflight(&s, f, ActionKind::Trash, &k).is_err());
    }
}

#[test]
fn toolchains_list_conda_environments_uv_pythons_and_ide_jdks() {
    let f = Fixture::new();
    let home = f.dir("home");
    f.write("home/miniconda3/envs/data/bin/python", "x");
    f.write("home/miniconda3/bin/python", "x");
    f.write(
        "home/.local/share/uv/python/cpython-3.12.4-macos-aarch64-none/bin/python3",
        "x",
    );
    f.write("home/.local/share/uv/python/.lock", "");
    f.write(
        "home/Library/Java/JavaVirtualMachines/corretto-17.0.8/Contents/Home/bin/java",
        "x",
    );
    let module = Arc::new(ToolchainsModule {
        home: Some(home),
        ..Default::default()
    });
    let report = Engine::new(services(&f), builtin(&f).register(module)).scan_report(
        &["toolchains".into()],
        &f.context(),
        &ScanControl::default(),
    );
    let mut rows: Vec<_> = report
        .findings
        .iter()
        .map(|f| {
            (
                f.title.as_str(),
                f.value("Official command").unwrap_or(""),
                f.risk,
            )
        })
        .collect();
    rows.sort_by_key(|(title, _, _)| title.to_string());
    assert_eq!(
        rows,
        [
            (
                "Conda environment data",
                "conda env remove -n data",
                Risk::Review
            ),
            ("JDK corretto-17.0.8", "", Risk::Review),
            (
                "Python cpython-3.12.4-macos-aarch64-none",
                "uv python uninstall cpython-3.12.4-macos-aarch64-none",
                Risk::Review
            ),
        ],
        "{:?}",
        report.warnings
    );
}
