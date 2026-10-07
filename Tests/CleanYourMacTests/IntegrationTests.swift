import CleanYourMacCore
import CleanYourMacModules
import CleanYourMacPlatform
import Foundation
import Testing

@Test func realGitWorktreeRejectsLateChangesAndRemovesOnlyEligibleTree() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let repository = try fixture.directory("main")
    _ = try fixture.write("main/readme.txt")
    let git = GitService()
    func command(_ arguments: [String], in path: String? = nil) async throws {
        let result = try await git.run(in: path ?? repository, arguments: arguments)
        #expect(result.status == 0, "\(arguments): \(result.error)")
        guard result.status == 0 else { throw CleanError.message(result.error) }
    }
    try await command(["init", "-b", "main"])
    try await command(["add", "readme.txt"])
    try await command(["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-m", "fixture"])
    try await command(["update-ref", "refs/remotes/origin/main", "HEAD"])
    try await command(["config", "remote.origin.url", repository])
    try await command(["config", "remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"])
    let linked = fixture.path + "/linked"
    try await command(["worktree", "add", "-b", "fixture-feature", linked, "main"])
    try await command(["branch", "--set-upstream-to=origin/main", "fixture-feature"])
    let record = try #require(await git.worktrees(repository).first { $0.path == linked })
    #expect(try await git.safety(record, in: repository).eligible)
    let context = ScanContext(roots: [fixture.path])
    let identity = try FileService().snapshotIdentity(linked)
    _ = try fixture.write("linked/late-untracked.txt")
    await #expect(throws: CleanError.self) { try await git.remove(file: identity, repository: repository, expectedHead: record.head, context: context) }
    #expect(FileManager.default.fileExists(atPath: linked))
    try FileManager.default.removeItem(atPath: linked + "/late-untracked.txt")
    try await git.remove(file: FileService().snapshotIdentity(linked), repository: repository, expectedHead: record.head, context: context)
    #expect(!FileManager.default.fileExists(atPath: linked))
    #expect(FileManager.default.fileExists(atPath: repository))
    let branch = try await git.run(in: repository, arguments: ["show-ref", "--verify", "refs/heads/fixture-feature"])
    #expect(branch.status == 0)
}

@Test func commandRunnerRejectsArbitraryExecutablesAndHandlesPreCancellation() async {
    await #expect(throws: CleanError.self) { try await CommandRunner().run("/bin/sh", ["-c", "true"], timeout: 1) }
    let task = Task {
        try Task.checkCancellation()
        return try await CommandRunner().run("/usr/bin/git", ["--version"], timeout: 2)
    }
    task.cancel()
    do { _ = try await task.value }
    catch { #expect(error is CancellationError) }
}

@Test func currentExclusionsAndOutOfScopeItemsBlockActions() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let path = try fixture.write("reviewed-file")
    let files = FileService()
    let finding = Finding(id: "fixture", moduleID: "large", title: "file", subtitle: path, resource: .file(try files.entry(path).identity), actions: [.trash], reason: "fixture")
    let executor = ActionExecutor(processes: ProcessService(), journal: ActionJournal(url: fixture.url.appendingPathComponent("history.json")))
    let excluded = await executor.execute(ActionRequest(findings: [finding], kind: .trash, context: ScanContext(roots: [fixture.path], exclusions: [path])))
    #expect(excluded.first?.outcome == .failed)
    let outside = await executor.execute(ActionRequest(findings: [finding], kind: .trash, context: ScanContext(roots: [fixture.path + "/elsewhere"])))
    #expect(outside.first?.outcome == .failed)
    #expect(FileManager.default.fileExists(atPath: path))
}

@Test func overlappingSelectionsAreNormalizedAndProcessInstancesStayDistinct() {
    func file(_ id: String, _ path: String) -> Finding {
        Finding(id: id, moduleID: "test", title: id, subtitle: path, resource: .file(FileIdentity(path: path, device: 1, inode: 1, modifiedSeconds: 0, modifiedNanos: 0)), reason: "fixture")
    }
    let parent = file("parent", "/projects/node_modules")
    let child = file("child", "/projects/node_modules/package/file")
    let peer = file("peer", "/projects/node_modules-other")
    #expect(Set(PathPolicy.normalizedSelection([parent, parent, child, peer]).map(\.id)) == ["parent", "peer"])
    let a = Finding(id: "process-1", moduleID: "orphans", title: "node", subtitle: "", resource: .process(process().identity), reason: "fixture")
    let b = Finding(id: "process-2", moduleID: "orphans", title: "node", subtitle: "", resource: .process(ProcessIdentity(pid: 43, uid: 501, startedSeconds: 101, startedMicroseconds: 1, executable: "/opt/homebrew/bin/node")), reason: "fixture")
    #expect(PathPolicy.normalizedSelection([a, b]).count == 2)
}

@Test func leftoverNamesExcludeAppleAndSharedContainers() {
    #expect(AppLeftoversModule.bundleIdentifier(from: "com.example.Editor.plist") == "com.example.Editor")
    #expect(AppLeftoversModule.bundleIdentifier(from: "com.example.Editor.savedState") == "com.example.Editor")
    #expect(AppLeftoversModule.bundleIdentifier(from: "com.apple.finder.plist") == nil)
    #expect(AppLeftoversModule.bundleIdentifier(from: "group.com.example.shared") == nil)
    #expect(AppLeftoversModule.bundleIdentifier(from: "Unidentified User Files") == nil)
}

private struct FailingModule: ScanModule {
    let descriptor = ModuleDescriptor(id: "failure", name: "Fixture", category: .tools, symbol: "tray", summary: "")
    func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        AsyncThrowingStream { $0.finish(throwing: CleanError.message("fixture unavailable")) }
    }
}
@Test func coordinatorReportsFailedIntegrationAsPartialCoverage() async throws {
    var warnings: [String] = []
    for try await event in ScanCoordinator(registry: ModuleRegistry([FailingModule()])).scan(moduleIDs: ["failure"], context: ScanContext(roots: [])) {
        if case .warning(let message) = event { warnings.append(message) }
    }
    #expect(warnings == ["Fixture: fixture unavailable"])
}

@Test(.enabled(if: ProcessInfo.processInfo.environment["CYM_NATIVE_TESTS"] == "1"))
func nativeTrashRoundTrip() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let path = try fixture.write("CleanYourMac-native-test-" + UUID().uuidString + ".txt", "Disposable native Trash fixture")
    let files = FileService()
    let identity = try files.snapshotIdentity(path)
    let destination = try files.moveToTrash(identity, context: ScanContext(roots: [fixture.path]))
    let result = ActionResult(title: "Native fixture", originalPath: path, action: .trash, outcome: .applied, message: "Fixture", trashPath: destination, trashIdentity: try files.snapshotIdentity(destination))
    // Restore only the exact generated fixture, even if an assertion fails.
    defer { if !FileManager.default.fileExists(atPath: path) { try? files.restore(result) } }
    #expect(!FileManager.default.fileExists(atPath: path))
    #expect(FileManager.default.fileExists(atPath: destination))
    try files.restore(result)
    #expect(try String(contentsOfFile: path, encoding: .utf8) == "Disposable native Trash fixture")
}
