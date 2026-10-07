import CleanYourMacCore
import CleanYourMacPlatform
import Foundation

public struct NodeModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "node", name: "Node Dependencies", category: .developer, symbol: "shippingbox", summary: "Find node_modules by project. Inspect local patches before reinstalling.")
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            let files = FileService()
            var candidates: [FileEntry] = []
            try files.walk(context: context, visit: { entry in
                if entry.isDirectory, (entry.path as NSString).lastPathComponent == "node_modules" { candidates.append(entry); return false }
                return true
            }, warning: { stream.yield(.warning($0)) })
            for entry in candidates {
                try Task.checkCancellation()
                let project = (entry.path as NSString).deletingLastPathComponent
                stream.yield(.progress("Sizing \(project)"))
                let manifest = FileManager.default.fileExists(atPath: project + "/package.json")
                let locks = [("pnpm", "pnpm-lock.yaml"), ("Yarn", "yarn.lock"), ("Bun", "bun.lock"), ("Bun", "bun.lockb"), ("npm", "package-lock.json")]
                let manager = locks.first { FileManager.default.fileExists(atPath: project + "/" + $0.1) }?.0 ?? "Unknown"
                var finding = try Streams.file(entry, module: "node", title: (project as NSString).lastPathComponent, reason: "Project dependencies can be reinstalled, but local patches or edits may be lost. Manifests and lockfiles are preserved.", actions: manifest ? [.trash] : [], risk: .rebuild, details: [Detail("Project", project), Detail("Package manager", manager), Detail("Artifact", "node_modules")])
                if !manifest { finding.blockedReason = "No owning package.json. This directory needs manual inspection." }
                stream.yield(.finding(finding))
            }
        }
    }
}

public struct ArtifactModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "artifacts", name: "Build Artifacts", category: .developer, symbol: "hammer", summary: "Known generated outputs in JavaScript, Swift, Rust, Python and Gradle projects.")
    public init() {}
    public static func rule(name: String, parent: String) -> String? {
        let exists: (String) -> Bool = { FileManager.default.fileExists(atPath: parent + "/" + $0) }
        if [".next", ".nuxt", ".turbo", ".parcel-cache", "coverage"].contains(name), exists("package.json") { return "JavaScript generated output" }
        if name == ".build", exists("Package.swift") { return "SwiftPM build output" }
        if name == "target", exists("Cargo.toml") { return "Cargo build output" }
        if name == "build", exists("build.gradle") || exists("build.gradle.kts") { return "Gradle build output" }
        if ["__pycache__", ".venv", "venv"].contains(name), exists("pyproject.toml") || exists("requirements.txt") || exists("setup.py") { return "Python generated environment or bytecode" }
        return nil
    }
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            var candidates: [(FileEntry, String)] = []
            try FileService().walk(context: context, visit: { entry in
                if (entry.path as NSString).lastPathComponent == "node_modules" { return false }
                if entry.isDirectory, let rule = Self.rule(name: (entry.path as NSString).lastPathComponent, parent: (entry.path as NSString).deletingLastPathComponent) {
                    candidates.append((entry, rule)); return false
                }
                return true
            }, warning: { stream.yield(.warning($0)) })
            for (entry, rule) in candidates {
                try Task.checkCancellation()
                stream.yield(.progress("Sizing \(entry.path)"))
                stream.yield(.finding(try Streams.file(entry, module: "artifacts", reason: rule + ". Rebuilding requires the project's tools and dependencies.", risk: .rebuild)))
            }
        }
    }
}

public struct WorktreeModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "worktrees", name: "Git Worktrees", category: .developer, symbol: "arrow.triangle.branch", summary: "Find registered worktrees, local changes, locks and upstream state.")
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            let files = FileService(), git = GitService()
            var repositories = Set<String>()
            for root in files.roots(context.roots) {
                if FileManager.default.fileExists(atPath: root + "/.git") || FileManager.default.fileExists(atPath: root + "/HEAD") && FileManager.default.fileExists(atPath: root + "/objects") { repositories.insert(root) }
            }
            try files.walk(context: context, visit: { entry in
                if entry.isDirectory {
                    if ["node_modules", ".build", "target"].contains((entry.path as NSString).lastPathComponent) { return false }
                    if FileManager.default.fileExists(atPath: entry.path + "/.git") || FileManager.default.fileExists(atPath: entry.path + "/HEAD") && FileManager.default.fileExists(atPath: entry.path + "/objects") { repositories.insert(entry.path) }
                }
                return true
            }, warning: { stream.yield(.warning($0)) })
            var commonDirectories = Set<String>()
            for repository in repositories.sorted() {
                try Task.checkCancellation()
                do {
                    let common = try await git.commonDirectory(repository)
                    guard commonDirectories.insert(common).inserted else { continue }
                    for record in try await git.worktrees(repository) {
                        stream.yield(.progress("Inspecting \(record.path)"))
                        guard !context.excludes(record.path) else { continue }
                        guard let entry = try? files.entry(record.path) else {
                            stream.yield(.finding(Finding(id: "worktrees:" + record.path, moduleID: "worktrees", title: (record.path as NSString).lastPathComponent, subtitle: record.path, resource: .worktreeRegistration(path: record.path, repository: repository), details: [Detail("Repository", common), Detail("Branch", record.branch), Detail("State", "Registered folder is missing"), Detail("Git metadata", record.prunable ? "Prunable" : "Retained")], reason: "Git retains this registration, but its folder is missing. Inspect repository metadata before pruning it manually.", blockedReason: "Missing worktree registrations are inspection only.", badge: "Orphaned")))
                            continue
                        }
                        let inScope = context.roots.contains { PathPolicy.contains(record.path, in: $0) }
                        let safety = inScope ? try await git.safety(record, in: repository) : WorktreeSafety(eligible: false, reason: "Outside scan roots", status: "Not inspected", upstream: "Not checked")
                        let size = inScope ? try files.size(record.path) : FileSize()
                        let fresh = try files.entry(record.path)
                        let reason = !inScope ? "Registered outside the chosen roots. Add its parent as a root to inspect it." : safety.reason
                        let finding = Finding(id: "worktrees:" + record.path, moduleID: "worktrees", title: (entry.path as NSString).lastPathComponent, subtitle: entry.path, resource: .worktree(file: fresh.identity.withTreeSignature(size.signature), repository: repository, head: record.head), bytes: inScope ? size.logical : nil, allocatedBytes: inScope ? size.allocated : nil, modifiedAt: entry.modified, details: [Detail("Repository", common), Detail("Branch", record.branch.isEmpty ? "Detached HEAD" : record.branch), Detail("HEAD", record.head), Detail("State", safety.status), Detail("Upstream (local ref)", safety.upstream), Detail("Type", record.main ? "Main" : "Linked")], actions: inScope && safety.eligible && size.complete && !record.prunable ? [.removeWorktree] : [], risk: .permanent, reason: reason, blockedReason: record.prunable ? "Git reports stale registration metadata; inspect manually." : inScope && safety.eligible && size.complete ? nil : reason, badge: record.prunable ? "Orphaned" : record.main ? "Main" : "Linked")
                        stream.yield(.finding(finding))
                    }
                } catch { stream.yield(.warning("Cannot fully inspect \(repository): \(error.localizedDescription)")) }
            }
        }
    }
}

public struct SimulatorModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "simulators", name: "Simulators", category: .developer, symbol: "iphone", summary: "Device app data and installed runtimes. Device actions leave runtimes intact.", usesRoots: false)
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            let inventory = try await SimulatorService().inventory()
            for (runtimeID, devices) in inventory.devices.sorted(by: { $0.key < $1.key }) {
                for device in devices {
                    try Task.checkCancellation()
                    let path = device.dataPath ?? NSHomeDirectory() + "/Library/Developer/CoreSimulator/Devices/" + device.udid + "/data"
                    guard context.allows(path) else { continue }
                    let size = try? FileService().size(path)
                    if size == nil || size?.complete == false { stream.yield(.warning("Device data size is incomplete for \(device.name). Device actions use fresh simctl state independently.")) }
                    let stopped = device.state == "Shutdown"
                    stream.yield(.finding(Finding(id: "simulators:" + device.udid, moduleID: "simulators", title: device.name, subtitle: runtimeID, resource: .simulator(id: device.udid, state: device.state), bytes: size?.logical, allocatedBytes: size?.allocated, details: [Detail("Identifier", device.udid), Detail("State", device.state), Detail("Available", device.isAvailable == true ? "Yes" : "No"), Detail("Data path", path)], actions: stopped ? [.resetSimulator, .deleteSimulator] : [], risk: .permanent, reason: "Device data includes installed apps, settings and test documents. Reset/delete cannot be undone.", blockedReason: stopped ? nil : "Shut down this simulator in Xcode before acting.")))
                }
            }
            for runtime in inventory.runtimes {
                if let path = runtime.bundlePath { guard context.allows(path) else { continue } }
                else if context.limitToRoots { continue }
                stream.yield(.finding(Finding(id: "runtime:" + runtime.identifier + ":" + (runtime.bundlePath ?? runtime.version ?? ""), moduleID: "simulators", title: runtime.name, subtitle: "Installed runtime", resource: .simulator(id: runtime.identifier, state: "Runtime"), details: [Detail("Version", runtime.version ?? "Unknown"), Detail("Location", runtime.bundlePath ?? "Managed by Xcode")], reason: "Runtime installations are managed separately in Xcode Settings → Components.", blockedReason: "Manage this runtime through Xcode's component manager.")))
            }
        }
    }
}

public struct XcodeModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "xcode", name: "Xcode Data", category: .developer, symbol: "wrench.and.screwdriver", summary: "DerivedData and device support; archives remain visible and protected.", usesRoots: false)
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            for root in KnownLocations.xcode where FileManager.default.fileExists(atPath: root) {
                for entry in Streams.children(root, continuation: stream) where context.allows(entry.path) {
                    try Task.checkCancellation()
                    stream.yield(.progress("Sizing \(entry.path)"))
                    let archive = root.hasSuffix("/Archives")
                    stream.yield(.finding(try Streams.file(entry, module: "xcode", reason: archive ? "Archives may contain irreplaceable release builds and dSYMs." : "Xcode regenerates this build/device-support data when needed.", actions: archive ? [] : [.trash], risk: archive ? .review : .rebuild)))
                }
            }
        }
    }
}

public struct OrphanModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "orphans", name: "Orphan Processes", category: .tools, symbol: "cpu", summary: "Suspected unmanaged processes left behind after their parent exited.", usesRoots: false)
    public let processes: ProcessService
    public init(processes: ProcessService) { self.processes = processes }
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            for (snapshot, cpu) in try await processes.candidates(ignoring: context.ignoredProcessNames) {
                let identity = snapshot.identity
                guard !context.excludes(identity.executable), snapshot.workingDirectory.map({ !context.excludes($0) }) ?? true,
                      !context.limitToRoots || context.allows(identity.executable) || snapshot.workingDirectory.map({ context.allows($0) }) == true else { continue }
                stream.yield(.finding(Finding(id: "orphans:" + identity.key, moduleID: "orphans", title: snapshot.name, subtitle: snapshot.workingDirectory ?? identity.executable, resource: .process(identity), cpuPercent: cpu, memoryBytes: snapshot.memoryBytes, details: [Detail("PID", String(identity.pid)), Detail("Started", Date(timeIntervalSince1970: Double(identity.startedSeconds)).formatted()), Detail("Working folder", snapshot.workingDirectory ?? "Unavailable"), Detail("Executable", identity.executable), Detail("Command", snapshot.command), Detail("CPU convention", "100% is one CPU core; refresh for a rate sample.")], actions: [.terminate, .forceQuit], risk: .permanent, reason: "Parent is launchd; no known managed job or running app owns it. Intentionally detached daemons can also match. Review before termination.")))
            }
        }
    }
}
