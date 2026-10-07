import CleanYourMacCore
import Foundation

public struct WorktreeRecord: Sendable, Equatable {
    public var path = ""
    public var head = ""
    public var branch = ""
    public var locked = false
    public var prunable = false
    public var bare = false
    public var main = false
    public init() {}
}

public struct WorktreeSafety: Sendable {
    public let eligible: Bool
    public let reason: String
    public let status: String
    public let upstream: String
    public init(eligible: Bool, reason: String, status: String, upstream: String) {
        self.eligible = eligible; self.reason = reason; self.status = status; self.upstream = upstream
    }
}

public struct GitService: Sendable {
    private let runner: any CommandRunning
    public init(runner: any CommandRunning = CommandRunner()) { self.runner = runner }
    public func run(in repository: String, arguments: [String]) async throws -> CommandOutput {
        try await runner.run("/usr/bin/git", ["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "core.untrackedCache=false", "-C", repository] + arguments, timeout: 20)
    }
    public func commonDirectory(_ repository: String) async throws -> String {
        let output = try await run(in: repository, arguments: ["rev-parse", "--path-format=absolute", "--git-common-dir"])
        guard output.status == 0 else { throw CleanError.message(output.error.isEmpty ? "Git cannot inspect this repository." : output.error) }
        return output.text.hasSuffix("\n") ? String(output.text.dropLast()) : output.text
    }
    public func worktrees(_ repository: String) async throws -> [WorktreeRecord] {
        let output = try await run(in: repository, arguments: ["worktree", "list", "--porcelain", "-z"])
        guard output.status == 0 else { throw CleanError.message(output.error) }
        return Self.parseWorktrees(output.data)
    }
    public static func parseWorktrees(_ data: Data) -> [WorktreeRecord] {
        var records: [WorktreeRecord] = [], current = WorktreeRecord()
        func flush() {
            if !current.path.isEmpty { current.main = records.isEmpty; records.append(current) }
            current = WorktreeRecord()
        }
        for token in String(decoding: data, as: UTF8.self).split(separator: "\0", omittingEmptySubsequences: false) {
            if token.isEmpty { flush(); continue }
            if token.hasPrefix("worktree ") {
                if !current.path.isEmpty { flush() }
                current.path = String(token.dropFirst(9))
            } else if token.hasPrefix("HEAD ") { current.head = String(token.dropFirst(5)) }
            else if token.hasPrefix("branch ") { current.branch = String(token.dropFirst(7)).replacingOccurrences(of: "refs/heads/", with: "") }
            else if token == "bare" { current.bare = true }
            else if token.hasPrefix("locked") { current.locked = true }
            else if token.hasPrefix("prunable") { current.prunable = true }
        }
        flush()
        return records
    }
    public func safety(_ record: WorktreeRecord, in repository: String) async throws -> WorktreeSafety {
        if record.main || record.bare { return WorktreeSafety(eligible: false, reason: "Main and bare repositories are protected.", status: "Protected", upstream: "—") }
        if record.locked { return WorktreeSafety(eligible: false, reason: "This worktree is locked.", status: "Locked", upstream: "—") }
        if record.branch.isEmpty { return WorktreeSafety(eligible: false, reason: "Detached HEAD needs manual inspection.", status: "Detached", upstream: "Unknown") }
        if FileManager.default.fileExists(atPath: record.path + "/.gitmodules") { return WorktreeSafety(eligible: false, reason: "Worktrees with submodules need manual inspection.", status: "Submodules", upstream: "Unknown") }
        let state = try await run(in: record.path, arguments: ["status", "--porcelain=v1", "-z", "--untracked-files=all", "--ignored"])
        guard state.status == 0 else { throw CleanError.message("Git status is unavailable.") }
        if !state.data.isEmpty { return WorktreeSafety(eligible: false, reason: "Contains changes, untracked or ignored files. Inspect before removal.", status: "Local files", upstream: "Not checked") }
        let upstream = try await run(in: record.path, arguments: ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"])
        guard upstream.status == 0 else { return WorktreeSafety(eligible: false, reason: "No verified upstream. Local commits may exist only here.", status: "Clean", upstream: "None") }
        let ahead = try await run(in: record.path, arguments: ["rev-list", "--count", "@{upstream}..HEAD"])
        guard ahead.status == 0, Int(ahead.text.trimmingCharacters(in: .whitespacesAndNewlines)) == 0 else { return WorktreeSafety(eligible: false, reason: "Has unpushed or unknown local commits.", status: "Clean", upstream: upstream.text.trimmingCharacters(in: .whitespacesAndNewlines)) }
        return WorktreeSafety(eligible: true, reason: "Clean linked worktree with no ignored files or commits ahead of its local upstream reference. Review before removal.", status: "Clean", upstream: upstream.text.trimmingCharacters(in: .whitespacesAndNewlines))
    }
    public func remove(file: FileIdentity, repository: String, expectedHead: String, context: ScanContext) async throws {
        try FileService().validate(file, context: context)
        let records = try await worktrees(repository)
        guard let record = records.first(where: { $0.path == file.path }), record.head == expectedHead else { throw CleanError.message("Worktree registration or HEAD changed.") }
        let status = try await safety(record, in: repository)
        guard status.eligible else { throw CleanError.message(status.reason) }
        let result = try await run(in: repository, arguments: ["worktree", "remove", "--", file.path])
        guard result.status == 0 else { throw CleanError.message(result.error) }
    }
}
