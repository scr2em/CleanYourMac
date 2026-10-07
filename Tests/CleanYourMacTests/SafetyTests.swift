import CleanYourMacCore
import CleanYourMacModules
import CleanYourMacPlatform
import Foundation
import Testing

struct Fixture {
    let url: URL
    init() throws {
        url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent(".test-fixtures/" + UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }
    var path: String { url.path }
    func write(_ name: String, _ contents: String = "fixture") throws -> String {
        let target = url.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(contents.utf8).write(to: target)
        return target.path
    }
    func directory(_ name: String) throws -> String {
        let target = url.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
        return target.path
    }
    func clean() { try? FileManager.default.removeItem(at: url) }
}

struct FixtureTrash: TrashMoving {
    let root: String
    func move(_ path: String) throws -> String {
        let target = root + "/" + UUID().uuidString
        try FileManager.default.moveItem(atPath: path, toPath: target)
        return target
    }
}

actor StubRunner: CommandRunning {
    let responses: [CommandOutput]
    var calls: [[String]] = []
    var index = 0
    init(_ responses: [CommandOutput]) { self.responses = responses }
    func run(_ executable: String, _ arguments: [String], timeout: TimeInterval) async throws -> CommandOutput {
        calls.append([executable] + arguments)
        guard index < responses.count else { throw CleanError.message("Unexpected command.") }
        defer { index += 1 }
        return responses[index]
    }
}
func output(_ text: String, status: Int32 = 0) -> CommandOutput { CommandOutput(data: Data(text.utf8), error: status == 0 ? "" : "fixture failure", status: status) }

func scan(_ module: any ScanModule, root: String) async throws -> [Finding] {
    var findings: [Finding] = []
    for try await event in module.scan(in: ScanContext(roots: [root])) {
        if case .finding(let finding) = event { findings.append(finding) }
    }
    return findings
}

@Test func pathBoundariesAndSensitiveItems() {
    #expect(PathPolicy.contains("/Projects/app/file", in: "/Projects/app"))
    #expect(!PathPolicy.contains("/Projects/application/file", in: "/Projects/app"))
    #expect(!PathPolicy.contains("/Projects/app/../../secret", in: "/Projects/app"))
    #expect(PathPolicy.protected("/Projects/app/.git/config"))
    #expect(PathPolicy.protected("/Projects/app/.env.local"))
    #expect(PathPolicy.protected(NSHomeDirectory() + "/.ssh/id_ed25519"))
    #expect(PathPolicy.protected(NSHomeDirectory() + "/Library/CloudStorage/provider/file"))
}

@Test func fileIdentityRejectsChangesAndSymlinkSwaps() throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let file = try fixture.write("data", "before")
    let files = FileService(), identity = try files.entry(file).identity
    try Data("after with a different size".utf8).write(to: URL(fileURLWithPath: file))
    #expect(throws: CleanError.self) { try files.validate(identity, context: ScanContext(roots: [fixture.path])) }
    try FileManager.default.removeItem(atPath: file)
    let other = try fixture.write("other")
    try FileManager.default.createSymbolicLink(atPath: file, withDestinationPath: other)
    #expect(throws: CleanError.self) { try files.validate(identity, context: ScanContext(roots: [fixture.path])) }
}

@Test func protectedDescendantBlocksRemovalOfItsParent() throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let folder = try fixture.directory("reviewed-folder")
    let protectedFile = try fixture.write("reviewed-folder/preserved.txt")
    let files = FileService()
    let context = ScanContext(roots: [fixture.path], exclusions: [protectedFile])
    #expect(context.protectsSelection(folder))
    #expect(throws: CleanError.self) { try files.validate(files.snapshotIdentity(folder), context: context) }
    #expect(FileManager.default.fileExists(atPath: protectedFile))
}

@Test func sizingDoesNotDoubleCountHardLinkAllocation() throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let original = try fixture.write("original", String(repeating: "x", count: 10_000))
    try FileManager.default.linkItem(atPath: original, toPath: fixture.path + "/linked")
    let size = try FileService().size(fixture.path)
    #expect(size.logical == 20_000)
    #expect(size.allocated == (try FileService().entry(original).allocated))
    #expect(size.files == 2)
}

@Test func aliasedAndOverlappingRootsAreScannedOnce() throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let project = try fixture.directory("project")
    let nested = try fixture.directory("project/nested")
    let alias = fixture.path + "/alias"
    try FileManager.default.createSymbolicLink(atPath: alias, withDestinationPath: project)
    #expect(FileService().roots([project, alias, nested]) == [project])
    let capitalized = fixture.path + "/PROJECT"
    if FileManager.default.fileExists(atPath: capitalized) {
        #expect(FileService().roots([project, capitalized]).count == 1)
    }
}

@Test func folderIdentityDetectsDeepChangesAndCountsLinksWithoutFollowingThem() throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let folder = try fixture.directory("artifact")
    let nested = try fixture.write("artifact/deep/file", "before")
    let external = try fixture.write("external/file", String(repeating: "x", count: 50_000))
    try FileManager.default.createSymbolicLink(atPath: folder + "/linked", withDestinationPath: external)
    let files = FileService()
    let size = try files.size(folder)
    #expect(size.complete)
    #expect(size.logical < 50_000)
    let identity = try files.snapshotIdentity(folder)
    try files.validate(identity, context: ScanContext(roots: [fixture.path]))
    try Data("changed inside nested folder".utf8).write(to: URL(fileURLWithPath: nested))
    #expect(throws: CleanError.self) { try files.validate(identity, context: ScanContext(roots: [fixture.path])) }
    #expect(FileManager.default.fileExists(atPath: external))
}

@Test func restoringChangedTrashFolderIsRejected() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let folder = try fixture.directory("personal")
    _ = try fixture.write("personal/deep/file")
    let trash = try fixture.directory("fixture-trash")
    let files = FileService(trash: FixtureTrash(root: trash))
    let finding = Finding(id: "folder", moduleID: "large", title: "personal", subtitle: folder, resource: .file(try files.snapshotIdentity(folder)), actions: [.trash], reason: "Fixture selection")
    let executor = ActionExecutor(processes: ProcessService(), journal: ActionJournal(url: fixture.url.appendingPathComponent("history.json")), files: files)
    let result = try #require(await executor.execute(ActionRequest(findings: [finding], kind: .trash, context: ScanContext(roots: [fixture.path]))).first)
    let trashPath = try #require(result.trashPath)
    try Data("changed".utf8).write(to: URL(fileURLWithPath: trashPath + "/deep/file"))
    await #expect(throws: CleanError.self) { try await executor.restore(result) }
    #expect(!FileManager.default.fileExists(atPath: folder))
}

@Test func nodeFinderGroupsNestedDependenciesAndRequiresOwner() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    _ = try fixture.write("app/package.json", "{}")
    _ = try fixture.write("app/pnpm-lock.yaml", "lockfileVersion: 9")
    _ = try fixture.write("app/node_modules/dependency/node_modules/nested/index.js")
    _ = try fixture.write("unknown/node_modules/index.js")
    let findings = try await scan(NodeModule(), root: fixture.path)
    #expect(findings.count == 2)
    #expect(findings.first { $0.title == "app" }?.actions == [.trash])
    #expect(findings.first { $0.title == "unknown" }?.actions.isEmpty == true)
}

@Test func artifactsRequireProjectEvidence() throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    #expect(ArtifactModule.rule(name: "build", parent: fixture.path) == nil)
    #expect(ArtifactModule.rule(name: "target", parent: fixture.path) == nil)
    _ = try fixture.write("Cargo.toml")
    #expect(ArtifactModule.rule(name: "target", parent: fixture.path) != nil)
    #expect(ArtifactModule.rule(name: "dist", parent: fixture.path) == nil)
}

@Test func duplicatesPreserveAnOriginalAndRevalidateContents() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    _ = try fixture.write("a.txt", "equal contents")
    _ = try fixture.write("b.txt", "equal contents")
    _ = try fixture.write("different.txt", "other contents")
    let findings = try await scan(DuplicateModule(minimumBytes: 1), root: fixture.path)
    #expect(findings.count == 2)
    #expect(findings.filter { $0.actions.isEmpty }.count == 1)
    let candidate = try #require(findings.first { !$0.actions.isEmpty })
    let original = try #require(candidate.details.first { $0.label == "Preserved original" }?.value)
    try Data("changed".utf8).write(to: URL(fileURLWithPath: original))
    let journal = ActionJournal(url: fixture.url.appendingPathComponent("journal.json"))
    let executor = ActionExecutor(processes: ProcessService(), journal: journal)
    let results = await executor.execute(ActionRequest(findings: [candidate], kind: .trash, context: ScanContext(roots: [fixture.path])))
    #expect(results.first?.outcome == .failed)
    #expect(FileManager.default.fileExists(atPath: candidate.resource.path!))
}

@Test func reviewedTrashAndRestoreUseExactResources() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let file = try fixture.write("personal.txt")
    let trash = try fixture.directory("fixture-trash")
    let files = FileService(trash: FixtureTrash(root: trash))
    let finding = Finding(id: "file", moduleID: "large", title: "personal.txt", subtitle: file, resource: .file(try files.entry(file).identity), actions: [.trash], reason: "Fixture selection")
    let journal = ActionJournal(url: fixture.url.appendingPathComponent("history.json"))
    let executor = ActionExecutor(processes: ProcessService(), journal: journal, files: files)
    let result = try #require(await executor.execute(ActionRequest(findings: [finding], kind: .trash, context: ScanContext(roots: [fixture.path]))).first)
    #expect(result.outcome == .applied)
    #expect(!FileManager.default.fileExists(atPath: file))
    #expect(result.trashIdentity != nil)
    try await executor.restore(result)
    #expect(try String(contentsOfFile: file, encoding: .utf8) == "fixture")
    #expect(await journal.read().count == 1)
}

@Test func worktreeParserPreservesUnusualPaths() {
    let rows = GitService.parseWorktrees(Data("worktree /projects/main\0HEAD abc\0branch refs/heads/main\0\0worktree /projects/line\nbreak\0HEAD def\0detached\0locked reason\0\0".utf8))
    #expect(rows.count == 2)
    #expect(rows[0].main)
    #expect(rows[1].path == "/projects/line\nbreak")
    #expect(rows[1].locked)
    #expect(rows[1].branch.isEmpty)
}

@Test func worktreeWithIgnoredFilesCannotBeRemoved() async throws {
    let runner = StubRunner([output("!! .env\0")])
    var record = WorktreeRecord(); record.path = "/fixture/worktree"; record.branch = "feature"
    let safety = try await GitService(runner: runner).safety(record, in: "/fixture/main")
    #expect(!safety.eligible)
    #expect(safety.reason.contains("ignored"))
    #expect(await runner.calls.count == 1)
}

@Test func simulatorActionsRequireShutdownAndSpecificID() async throws {
    let id = "35D71E75-E2F0-4EF5-A131-06B14FDB044E"
    let json = """
    {"devices":{"runtime":[{"udid":"\(id)","name":"Fixture","state":"Shutdown","isAvailable":true}]},"runtimes":[]}
    """
    let runner = StubRunner([output(json), output("")])
    try await SimulatorService(runner: runner).act(id: id, action: .resetSimulator)
    #expect(await runner.calls.last == ["/usr/bin/xcrun", "simctl", "erase", id])
    let booted = StubRunner([output(json.replacingOccurrences(of: "Shutdown", with: "Booted"))])
    await #expect(throws: CleanError.self) { try await SimulatorService(runner: booted).act(id: id, action: .deleteSimulator) }
    #expect(await booted.calls.count == 1)
    await #expect(throws: CleanError.self) { try await SimulatorService(runner: runner).act(id: "all", action: .resetSimulator) }
    #expect(await runner.calls.count == 2)
}

func process(path: String = "/opt/homebrew/bin/node", parent: Int32 = 1, uid: UInt32 = 501) -> ProcessSnapshot {
    ProcessSnapshot(identity: ProcessIdentity(pid: 42, uid: uid, startedSeconds: 100, startedMicroseconds: 2, executable: path), parent: parent, name: (path as NSString).lastPathComponent, command: "fixture", workingDirectory: nil, cpuNanos: 0, memoryBytes: 0)
}

@Test func orphanClassificationExcludesServicesAppsAndOtherUsers() {
    func reason(_ snapshot: ProcessSnapshot, managed: Set<Int32> = [], apps: Set<Int32> = [], bundles: Set<String> = [], ignored: Set<String> = []) -> String? {
        OrphanClassifier.exclusion(process: snapshot, uid: 501, ownPID: 99, managed: managed, appPIDs: apps, appBundles: bundles, ignored: ignored)
    }
    #expect(reason(process()) == nil)
    #expect(reason(process(), managed: [42]) != nil)
    #expect(reason(process(), apps: [42]) != nil)
    #expect(reason(process(uid: 502)) != nil)
    #expect(reason(process(parent: 10)) != nil)
    #expect(reason(process(path: "/System/Library/tool")) != nil)
    #expect(reason(process(path: "/Applications/Editor.app/Contents/helper"), bundles: ["/Applications/Editor.app"]) != nil)
    #expect(reason(process(path: "/Applications/Editor.app/Contents/service.xpc/worker")) != nil)
    #expect(reason(process(path: "/opt/homebrew/bin/ssh-agent"), ignored: ["ssh-agent"]) != nil)
}

@Test func unknownManagedJobsFailClosedAndForceRequiresGracefulRequest() async {
    let runner = StubRunner([output("", status: 1)])
    let service = ProcessService(runner: runner)
    await #expect(throws: CleanError.self) { try await service.candidates(ignoring: []) }
    await #expect(throws: CleanError.self) { try await service.signal(process().identity, force: true, ignoring: []) }
    #expect(await runner.calls.count == 1)
}

@Test func processIdentityIncludesSubsecondStartTime() {
    let a = ProcessIdentity(pid: 42, uid: 501, startedSeconds: 100, startedMicroseconds: 1, executable: "/bin/tool")
    let b = ProcessIdentity(pid: 42, uid: 501, startedSeconds: 100, startedMicroseconds: 2, executable: "/bin/tool")
    #expect(a != b)
    #expect(a.key != b.key)
}
