import CleanYourMacCore
import Foundation
import Testing

private func collect(_ stream: AsyncThrowingStream<ScanEvent, Error>) async throws -> (findings: [Finding], warnings: [String]) {
    var findings: [Finding] = [], warnings: [String] = []
    for try await event in stream {
        switch event {
        case .finding(let finding): findings.append(finding)
        case .warning(let warning): warnings.append(warning)
        default: break
        }
    }
    return (findings, warnings)
}

@Test func coreDescribesRegisteredModules() {
    let modules = CoreEngine.shared.modules()
    #expect(modules.count == 15)
    #expect(modules.first?.id == "storage")
    #expect(modules.first { $0.id == "orphans" }?.category == .tools)
    #expect(modules.first { $0.id == "toolchains" }?.category == .developer)
    #expect(Set(modules.map(\.id)).count == modules.count)
}

@Test func scanStreamDecodesFindingsFromTheCore() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    try fixture.write("app/package.json", "{}")
    try fixture.write("app/node_modules/dependency/index.js")
    let result = try await collect(CoreEngine.shared.scan(moduleIDs: ["node"], context: ScanContext(roots: [fixture.path])))
    #expect(result.warnings.isEmpty)
    let finding = try #require(result.findings.first)
    #expect(result.findings.count == 1)
    #expect(finding.moduleID == "node")
    #expect(finding.actions == [.trash])
    #expect(finding.value("Project") == fixture.path + "/app")
    guard case .file(let identity) = finding.resource else { Issue.record("Expected a file resource"); return }
    #expect(identity.treeSignature != nil)
    #expect(finding.modifiedAt.map { abs($0.timeIntervalSinceNow) < 3_600 } == true)
}

@Test func findingsRoundTripThroughTheCore() {
    let identity = FileIdentity(path: "/projects/node_modules", device: 1, inode: 2, modifiedSeconds: 3, modifiedNanos: 4, treeSignature: "1:ab")
    let parent = Finding(id: "parent", moduleID: "node", title: "parent", subtitle: "", resource: .file(identity), modifiedAt: Date(timeIntervalSince1970: 1_700_000_000), actions: [.trash], reason: "fixture")
    let child = Finding(id: "child", moduleID: "node", title: "child", subtitle: "", resource: .file(FileIdentity(path: "/projects/node_modules/a", device: 1, inode: 3, modifiedSeconds: 0, modifiedNanos: 0)), reason: "fixture")
    let process = Finding(id: "process", moduleID: "orphans", title: "node", subtitle: "", resource: .process(ProcessIdentity(pid: 42, uid: 501, startedSeconds: 1, startedMicroseconds: 2, executable: "/bin/node")), reason: "fixture")
    let worktree = Finding(id: "worktree", moduleID: "worktrees", title: "w", subtitle: "", resource: .worktree(file: identity, repository: "/r", head: "abc"), reason: "fixture")
    let rows = CoreEngine.shared.normalizedSelection([parent, child, process, worktree])
    #expect(rows.map(\.id).sorted() == ["parent", "process", "worktree"])
    #expect(rows.first { $0.id == "parent" } == parent)
    #expect(rows.first { $0.id == "worktree" }?.resource == worktree.resource)
}

@Test func actionsFailClosedAndAreJournaled() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let core = CoreEngine(journalPath: fixture.path + "/history.json")
    let path = try fixture.write("reviewed-file")
    let scanned = try await collect(core.scan(moduleIDs: ["storage"], context: ScanContext(roots: [fixture.path])))
    var finding = try #require(scanned.findings.first { $0.resource.path == path })
    finding.actions = [.trash]
    let excluded = try await core.execute(ActionRequest(findings: [finding], kind: .trash, context: ScanContext(roots: [fixture.path], exclusions: [path])))
    #expect(excluded.first?.outcome == .failed)
    #expect(FileManager.default.fileExists(atPath: path))
    let history = await core.history()
    #expect(history.count == 1)
    #expect(history.first.map { abs($0.date.timeIntervalSinceNow) < 60 } == true)
    try await core.clearHistory()
    #expect(await core.history().isEmpty)
}

@Test func analyticsCountOverlappingPathsOnce() async throws {
    func file(_ id: String, _ module: String, _ path: String, _ bytes: UInt64) -> Finding {
        Finding(id: id, moduleID: module, title: id, subtitle: path, resource: .file(FileIdentity(path: path, device: 1, inode: 1, modifiedSeconds: 0, modifiedNanos: 0)), bytes: bytes, actions: [.trash], reason: "fixture")
    }
    let totals = await CoreEngine.shared.analytics([file("a", "storage", "/p", 100), file("b", "node", "/p/node_modules", 60), file("c", "large", "/q", 5)])
    #expect(totals.diskBytes == 105)
    #expect(totals.modules.count == 3)
}

@Test func cancellingTheConsumerStopsTheScan() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    for index in 0..<50 { try fixture.write("tree/\(index)/file") }
    let task = Task { try await collect(CoreEngine.shared.scan(moduleIDs: ["storage", "large", "duplicates"], context: ScanContext(roots: [fixture.path]))) }
    task.cancel()
    _ = try? await task.value
}

@Test(.enabled(if: ProcessInfo.processInfo.environment["CYM_NATIVE_TESTS"] == "1"))
func nativeTrashRoundTrip() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    let core = CoreEngine(journalPath: fixture.path + "/history.json")
    let name = "CleanYourMac-native-test-" + UUID().uuidString + ".txt"
    let path = try fixture.write(name, "Disposable native Trash fixture")
    let scanned = try await collect(core.scan(moduleIDs: ["storage"], context: ScanContext(roots: [fixture.path])))
    var finding = try #require(scanned.findings.first { $0.resource.path == path })
    finding.actions = [.trash]
    let result = try #require(try await core.execute(ActionRequest(findings: [finding], kind: .trash, context: ScanContext(roots: [fixture.path]))).first)
    // Restore only the exact generated fixture, even if an assertion fails.
    defer { if !FileManager.default.fileExists(atPath: path) { Task { try? await core.restore(result) } } }
    #expect(result.outcome == .applied, "\(result.message)")
    #expect(!FileManager.default.fileExists(atPath: path))
    try await core.restore(result)
    #expect(try String(contentsOfFile: path, encoding: .utf8) == "Disposable native Trash fixture")
}
