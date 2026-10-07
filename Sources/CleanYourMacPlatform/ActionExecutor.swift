import AppKit
import CleanYourMacCore
import Foundation

public actor ActionJournal {
    public static let shared = ActionJournal()
    private let url: URL
    public init(url: URL? = nil) {
        self.url = url ?? URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support/CleanYourMac/activity.json")
    }
    public func read() -> [ActionResult] {
        guard let data = try? Data(contentsOf: url), let rows = try? JSONDecoder().decode([ActionResult].self, from: data) else { return [] }
        return rows
    }
    public func append(_ results: [ActionResult]) throws {
        let rows = Array((results + read()).prefix(1_000))
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try JSONEncoder().encode(rows).write(to: url, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
    }
    public func clear() throws {
        if FileManager.default.fileExists(atPath: url.path) { try FileManager.default.removeItem(at: url) }
    }
}

public actor ActionExecutor {
    private let files: FileService
    private let git = GitService()
    private let simulators = SimulatorService()
    private let processes: ProcessService
    private let journal: ActionJournal
    public init(processes: ProcessService, journal: ActionJournal = .shared, files: FileService = FileService()) { self.processes = processes; self.journal = journal; self.files = files }

    public func execute(_ request: ActionRequest) async -> [ActionResult] {
        var results: [ActionResult] = []
        for finding in PathPolicy.normalizedSelection(request.findings) {
            if Task.isCancelled { break }
            var result: ActionResult
            do {
                guard finding.actions.contains(request.kind), finding.blockedReason == nil else { throw CleanError.message("The requested action is not eligible for this finding.") }
                if let path = finding.resource.path, request.context.protectsSelection(path) { throw CleanError.message("The item or an item inside it is now excluded.") }
                if request.context.limitToRoots {
                    let path = finding.details.first { $0.label == "Data path" }?.value ?? finding.resource.path
                    let working = finding.details.first { $0.label == "Working folder" }?.value
                    guard path.map({ request.context.allows($0) }) == true || (finding.moduleID == "orphans" && working.map({ request.context.allows($0) }) == true) else { throw CleanError.message("This item is outside the finder's current included paths.") }
                }
                if let working = finding.details.first(where: { $0.label == "Working folder" })?.value, request.context.excludes(working) { throw CleanError.message("This process working folder is now excluded.") }
                if let dataPath = finding.details.first(where: { $0.label == "Data path" })?.value, request.context.protectsSelection(dataPath) { throw CleanError.message("This simulator data path is now excluded.") }
                result = try await apply(finding, request: request)
            } catch {
                result = ActionResult(title: finding.title, originalPath: finding.resource.path, action: request.kind, outcome: .failed, message: error.localizedDescription, findingID: finding.id)
            }
            // Persist after each result so an interrupted batch retains its completed outcomes.
            do { try await journal.append([result]) }
            catch { result.journalWarning = "Action finished, but local history could not be saved: " + error.localizedDescription }
            results.append(result)
        }
        return results
    }

    private func apply(_ finding: Finding, request: ActionRequest) async throws -> ActionResult {
        switch (request.kind, finding.resource) {
        case (.trash, .file(let file)):
            if ["node", "artifacts"].contains(finding.moduleID) {
                let active = ProcessService.activeTools(under: (file.path as NSString).deletingLastPathComponent)
                guard active.isEmpty else { throw CleanError.message("Close active project tools first: " + active.prefix(3).joined(separator: ", ")) }
            }
            if finding.moduleID == "caches" {
                let active = ProcessService.activeTools()
                guard active.isEmpty else { throw CleanError.message("Close active developer tools before clearing shared caches: " + active.prefix(3).joined(separator: ", ")) }
            }
            if finding.moduleID == "xcode" {
                let running = await MainActor.run { NSWorkspace.shared.runningApplications.contains { $0.bundleIdentifier == "com.apple.dt.Xcode" } }
                let building = ProcessService.activeTools().contains { $0.hasPrefix("xcodebuild ") || $0.hasPrefix("swift-frontend ") }
                guard !running && !building else { throw CleanError.message("Quit Xcode and stop builds before removing its data.") }
            }
            if finding.moduleID == "applications" || finding.moduleID == "leftovers" {
                let owner = finding.details.first { $0.label == "Application path" }?.value ?? file.path
                let bundleID = finding.details.first { $0.label == "Bundle identifier" }?.value
                let running = await MainActor.run { NSWorkspace.shared.runningApplications.contains { $0.bundleURL?.path == owner || (bundleID != nil && $0.bundleIdentifier == bundleID) } }
                if running { throw CleanError.message("Quit this application before removing it.") }
            }
            if finding.moduleID == "duplicates" {
                guard let original = finding.details.first(where: { $0.label == "Preserved original" })?.value,
                      let expectedHash = finding.details.first(where: { $0.label == "SHA256" })?.value,
                      original != file.path,
                      try files.hash(original) == expectedHash, try files.hash(file.path) == expectedHash else {
                    throw CleanError.message("Duplicate content or preserved original changed. Scan again.")
                }
            }
            if finding.moduleID == "node" || finding.moduleID == "artifacts" {
                let directory = (file.path as NSString).deletingLastPathComponent
                var ancestor = directory
                var repositoryFound = false
                while ancestor != "/" {
                    if FileManager.default.fileExists(atPath: ancestor + "/.git") { repositoryFound = true; break }
                    ancestor = (ancestor as NSString).deletingLastPathComponent
                }
                if repositoryFound {
                    let state = try await git.run(in: directory, arguments: ["ls-files", "-z", "--", file.path])
                    guard state.status == 0, state.data.isEmpty else { throw CleanError.message("Artifact contains tracked files or Git inspection failed.") }
                }
                if finding.moduleID == "node", !FileManager.default.fileExists(atPath: directory + "/package.json") {
                    throw CleanError.message("The owning package.json is no longer available.")
                }
            }
            let destination = try files.moveToTrash(file, context: request.context, additionalRoots: KnownLocations.roots(for: finding.moduleID))
            return result(finding, request.kind, .applied, "Moved to Trash; storage is still occupied until Trash is emptied.", destination)
        case (.emptyTrash, .file(let file)):
            guard PathPolicy.contains(file.path, in: NSHomeDirectory() + "/.Trash"), file.path != NSHomeDirectory() + "/.Trash" else { throw CleanError.message("Only reviewed items inside your Trash are eligible.") }
            try files.validate(file, context: request.context, additionalRoots: [NSHomeDirectory() + "/.Trash"])
            try FileManager.default.removeItem(atPath: file.path)
            return result(finding, request.kind, .applied, "Permanently removed the reviewed Trash item.")
        case (.removeWorktree, .worktree(let file, let repository, let head)):
            try await git.remove(file: file, repository: repository, expectedHead: head, context: request.context)
            return result(finding, request.kind, .applied, "Removed linked worktree; branch retained.")
        case (.resetSimulator, .simulator(let id, _)), (.deleteSimulator, .simulator(let id, _)):
            try await simulators.act(id: id, action: request.kind)
            return result(finding, request.kind, .applied, request.kind == .resetSimulator ? "Simulator reset." : "Simulator deleted.")
        case (.terminate, .process(let identity)), (.forceQuit, .process(let identity)):
            let outcome = try await processes.signal(identity, force: request.kind == .forceQuit, ignoring: request.context.ignoredProcessNames)
            let message = outcome == .applied ? "Observed process exit." : outcome == .skipped ? "Process already exited." : "Signal delivered; process remains alive. Force Quit requires a separate review."
            return result(finding, request.kind, outcome, message)
        default: throw CleanError.message("Action and resource type do not match.")
        }
    }
    private func result(_ finding: Finding, _ action: ActionKind, _ outcome: Outcome, _ message: String, _ trash: String? = nil) -> ActionResult {
        // Process command arguments are deliberately omitted from the journal.
        ActionResult(title: finding.title, originalPath: finding.resource.path, action: action, outcome: outcome, message: message, trashPath: trash, findingID: finding.id, trashIdentity: trash.flatMap { try? files.snapshotIdentity($0) })
    }
    public func restore(_ result: ActionResult) async throws { try files.restore(result) }
}
