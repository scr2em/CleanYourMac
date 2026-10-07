import AppKit
import CleanYourMacCore
import CleanYourMacDesignSystem
import Foundation
import Observation

public struct ReviewDraft: Identifiable {
    public let id = UUID()
    public let request: ActionRequest
}
public enum SortOrder: String, CaseIterable { case size = "Size", name = "Name", cpu = "CPU" }
public enum SizeFilter: String, CaseIterable {
    case all = "Any size", large = "100 MB+", huge = "1 GB+"
    var minimum: UInt64 { switch self { case .all: 0; case .large: 100_000_000; case .huge: 1_000_000_000 } }
}
public enum AgeFilter: String, CaseIterable {
    case all = "Any age", months = "Older than 90 days", year = "Older than a year"
    var days: Double? { switch self { case .all: nil; case .months: 90; case .year: 365 } }
}

@MainActor @Observable
public final class AppStore {
    public let core: CoreEngine
    public let modules: [ModuleDescriptor]
    public var selectedModuleID: String? = "overview" {
        didSet {
            if selectedModuleID != oldValue {
                selectedIDs = []; inspectedID = nil; search = ""; sizeFilter = .all; ageFilter = .all
                if !isScanning && !demo {
                    progress = scanStatuses[selectedModuleID ?? "overview"] ?? "Ready · scan this tool"
                    warnings = moduleWarnings[selectedModuleID ?? "overview"] ?? []
                }
            }
        }
    }
    public var roots: [String]
    public var exclusions: [String]
    public var ignoredNames: [String]
    public var disabledModules: [String]
    public var findings: [Finding] = []
    public var selectedIDs = Set<String>()
    public var inspectedID: String?
    public var search = ""
    /// Size and CPU start largest first and Name starts A–Z; the direction can then be flipped.
    public var sort: SortOrder = .size { didSet { if sort != oldValue { sortAscending = sort == .name } } }
    public var sortAscending = false
    public var sizeFilter: SizeFilter = .all
    public var ageFilter: AgeFilter = .all
    public var isScanning = false
    public var isApplying = false
    public var progress = "Ready"
    public var warnings: [String] = []
    public var history: [ActionResult] = []
    public var review: ReviewDraft?
    public var error: String?
    public var showSettings = false
    public var showInspector = true
    public var menuBarEnabled = false { didSet { persist(); configureMonitor() } }
    /// The Comfy palette; colors resolve against it at draw time.
    public var theme: ComfyTheme = ComfyTheme.current { didSet { ComfyTheme.current = theme; persist() } }
    public var forceEligible = Set<String>()
    /// Core-computed totals for the current findings; refreshed after scans and actions.
    public var analytics = Analytics.empty
    /// Core-computed totals for the rows currently matching the search and filters.
    public var summary = Analytics.empty
    /// Changes whenever the visible rows can change; drives summary refreshes.
    public var summaryKey: String {
        "\(selectedModuleID ?? "")|\(search)|\(sizeFilter.rawValue)|\(ageFilter.rawValue)|\(findings.count)|\(storageNavigation.count)"
    }
    public var restoredIDs = Set<UUID>()
    public var storageNavigation: [String] = []
    private var scanStatuses: [String: String] = [:]
    private var moduleWarnings: [String: [String]] = [:]
    private var scanningModules: [String] = []
    public let demo: Bool
    private var scanTask: Task<Void, Never>?
    private var scanID: UUID?
    private var scanHadWarnings = false
    private var monitorTask: Task<Void, Never>?
    private var isRefreshingOrphans = false
    private var resultLimitReached = false
    private let defaults: UserDefaults

    public init(demo: Bool = false, defaults: UserDefaults = .standard, core: CoreEngine = .shared) {
        self.defaults = defaults; self.demo = demo; self.core = core
        modules = core.modules()
        let initial = core.projectRoots()
        roots = core.normalizeRoots(defaults.stringArray(forKey: "scanRoots") ?? (initial.isEmpty ? [NSHomeDirectory() + "/Downloads"] : initial))
        exclusions = defaults.stringArray(forKey: "excludedPaths") ?? []
        ignoredNames = defaults.stringArray(forKey: "ignoredProcessNames") ?? ["ssh-agent", "gpg-agent", "keyboxd", "dirmngr"]
        disabledModules = defaults.stringArray(forKey: "disabledModules") ?? []
        menuBarEnabled = !demo && defaults.bool(forKey: "menuBarEnabled")
        theme = defaults.string(forKey: "theme").flatMap(ComfyTheme.init(rawValue:)) ?? .honey
        ComfyTheme.current = theme
        configureMonitor()
        if demo { roots = ["/Users/demo/Projects"]; exclusions = []; disabledModules = []; loadDemo() }
    }
    public var context: ScanContext { ScanContext(roots: selectedModuleID == "storage" && !storageNavigation.isEmpty ? [storageNavigation.last!] : roots, exclusions: exclusions, ignoredProcessNames: ignoredNames) }
    public var enabledModules: [ModuleDescriptor] { modules.filter { !disabledModules.contains($0.id) } }
    public var currentModule: ModuleDescriptor? { modules.first { $0.id == selectedModuleID } }
    public var visibleFindings: [Finding] {
        let rows = findings.filter { finding in
            (selectedModuleID == "overview" || finding.moduleID == selectedModuleID)
            && (search.isEmpty || (finding.title + " " + finding.subtitle).localizedCaseInsensitiveContains(search))
            && (sizeFilter == .all || (finding.bytes ?? 0) >= sizeFilter.minimum)
            && (ageFilter.days == nil || finding.modifiedAt.map { $0 < Date().addingTimeInterval(-ageFilter.days! * 86_400) } == true)
        }
        if isScanning { return rows }
        let sort = self.sort
        func less(_ a: Finding, _ b: Finding) -> Bool {
            switch sort {
            case .size: return (a.bytes ?? a.memoryBytes ?? 0, a.id) < (b.bytes ?? b.memoryBytes ?? 0, b.id)
            case .cpu: return (a.cpuPercent ?? 0, a.id) < (b.cpuPercent ?? 0, b.id)
            case .name:
                let order = a.title.localizedStandardCompare(b.title)
                return order == .orderedSame ? a.id < b.id : order == .orderedAscending
            }
        }
        return sortAscending ? rows.sorted(by: less) : rows.sorted { less($1, $0) }
    }
    public var selectedFindings: [Finding] { findings.filter { selectedIDs.contains($0.id) } }
    public var inspected: Finding? { findings.first { $0.id == inspectedID } }
    public var availableActions: [ActionKind] {
        guard let first = selectedFindings.first else { return [] }
        return first.actions.filter { kind in
            selectedFindings.allSatisfy { $0.actions.contains(kind) && $0.blockedReason == nil && (kind != .forceQuit || forceEligible.contains($0.id)) }
        }
    }
    public var diskBytes: UInt64 { visibleFindings.filter { if case .process = $0.resource { false } else { true } }.reduce(0) { $0 + ($1.bytes ?? 0) } }
    public func persist() {
        guard !demo else { return }
        defaults.set(roots, forKey: "scanRoots"); defaults.set(exclusions, forKey: "excludedPaths")
        defaults.set(ignoredNames, forKey: "ignoredProcessNames"); defaults.set(disabledModules, forKey: "disabledModules")
        defaults.set(menuBarEnabled, forKey: "menuBarEnabled"); defaults.set(theme.rawValue, forKey: "theme")
    }
    public func addRoot() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false; panel.canChooseDirectories = true; panel.allowsMultipleSelection = true
        panel.message = "Choose folders to inspect. Scanning never removes files."
        if panel.runModal() == .OK {
            roots = core.normalizeRoots(roots + panel.urls.map(\.path)); storageNavigation = []; persist()
        }
    }
    public func select(_ id: String, checked: Bool) {
        if checked { selectedIDs.insert(id) } else { selectedIDs.remove(id) }
    }
    public func scan() {
        guard !isApplying else { return }
        if demo { loadDemo(); return }
        if isScanning { cancelScan() }
        scanTask?.cancel()
        let id = UUID(), selected = selectedModuleID ?? "overview", context = self.context
        scanID = id; isScanning = true; progress = "Starting scan…"; warnings = []; scanHadWarnings = false; resultLimitReached = false; selectedIDs = []; inspectedID = nil
        let ids = selected == "overview" ? enabledModules.map(\.id) : [selected]
        scanningModules = Array(Set(ids + [selected]))
        findings.removeAll { ids.contains($0.moduleID) }
        let stream = core.scan(moduleIDs: ids, context: context)
        scanTask = Task { [weak self] in
            guard let self else { return }
            do {
                var seen = Set(self.findings.map(\.id))
                for try await event in stream {
                    guard self.scanID == id else { return }
                    try Task.checkCancellation()
                    switch event {
                    case .finding(let finding):
                        if self.findings.count >= 30_000 {
                            if !self.resultLimitReached { self.scanHadWarnings = true; self.resultLimitReached = true; self.warnings.append("Result limit reached; scan narrower roots.") }
                            break
                        }
                        if seen.insert(finding.id).inserted { self.findings.append(finding) }
                    case .progress(let message): self.progress = message
                    case .warning(let message):
                        self.scanHadWarnings = true
                        if self.warnings.count < 200 { self.warnings.append(message) }
                    case .moduleFinished: break
                    }
                }
                if self.scanID == id {
                    let status = self.scanHadWarnings ? "Partial scan · review warnings" : "Scan complete"
                    self.finishScanStatus(status)
                }
            } catch {
                if self.scanID == id { self.finishScanStatus(error is CancellationError ? "Scan cancelled · partial results" : "Scan failed"); if !(error is CancellationError) { self.error = error.localizedDescription } }
            }
        }
    }
    private func finishScanStatus(_ status: String) {
        for id in scanningModules { scanStatuses[id] = status; moduleWarnings[id] = warnings }
        isScanning = false
        progress = scanStatuses[selectedModuleID ?? "overview"] ?? "Ready · scan this tool"
        warnings = moduleWarnings[selectedModuleID ?? "overview"] ?? []
        scanningModules = []
        refreshAnalytics()
    }
    public func refreshSummary() async {
        let rows = visibleFindings
        let totals = await core.analytics(rows)
        if visibleFindings.count == rows.count { summary = totals }
    }
    public func refreshAnalytics() {
        let rows = findings
        Task { [weak self] in
            guard let self else { return }
            let totals = await self.core.analytics(rows)
            if self.findings.count == rows.count { self.analytics = totals }
        }
    }
    public func cancelScan() { scanTask?.cancel(); scanID = nil; finishScanStatus("Scan cancelled · partial results") }
    public func browse(_ path: String) {
        guard !demo, roots.contains(where: { core.contains(path, in: $0) }), !exclusions.contains(where: { core.contains(path, in: $0) }) else { return }
        storageNavigation.append(path); scan()
    }
    public func browseBack() {
        guard !storageNavigation.isEmpty else { return }
        storageNavigation.removeLast(); scan()
    }
    public func reviewSelection(_ kind: ActionKind) { review = ReviewDraft(request: ActionRequest(findings: selectedFindings, kind: kind, context: context)) }
    public func apply(_ draft: ReviewDraft) async {
        guard !demo else { error = "Demo mode cannot modify files or processes."; return }
        isApplying = true
        let freshRequest = ActionRequest(findings: draft.request.findings, kind: draft.request.kind, context: self.context)
        let results: [ActionResult]
        do { results = try await core.execute(freshRequest) }
        catch { self.error = error.localizedDescription; isApplying = false; return }
        for result in results {
            if let id = result.findingID {
                if result.outcome == .requested && result.action == .terminate { forceEligible.insert(id) }
                if result.outcome == .applied || result.outcome == .skipped {
                    findings.removeAll { $0.id == id }; selectedIDs.remove(id); forceEligible.remove(id)
                }
            }
        }
        mergeHistory(results + (await core.history()))
        isApplying = false; review = nil
        refreshAnalytics()
        if let failed = results.first(where: { $0.outcome == .failed }) { error = failed.message }
        else if let warning = results.compactMap(\.journalWarning).first { error = warning }
    }
    private func mergeHistory(_ rows: [ActionResult]) {
        var seen = Set<UUID>()
        history = (rows + history).filter { seen.insert($0.id).inserted }.sorted { $0.date > $1.date }
    }
    public func loadHistory() async { mergeHistory(await core.history()) }
    public func clearHistory() async {
        do { try await core.clearHistory(); history = []; restoredIDs = [] }
        catch { self.error = error.localizedDescription }
    }
    private func configureMonitor() {
        monitorTask?.cancel()
        guard menuBarEnabled, !demo else { return }
        monitorTask = Task { [weak self] in
            while !Task.isCancelled {
                if let self, self.selectedModuleID != "orphans" { await self.refreshOrphans() }
                try? await Task.sleep(for: .seconds(10))
            }
        }
    }
    public func refreshOrphans() async {
        guard !demo, !isScanning, !isApplying, !isRefreshingOrphans else { return }
        isRefreshingOrphans = true
        defer { isRefreshingOrphans = false }
        do {
            var refreshed: [Finding] = []
            for try await event in core.scan(moduleIDs: ["orphans"], context: context) {
                if case .finding(let finding) = event { refreshed.append(finding) }
            }
            let freshIDs = Set(refreshed.map(\.id))
            findings.removeAll { $0.moduleID == "orphans" }; findings.append(contentsOf: refreshed)
            selectedIDs = selectedIDs.filter { id in !id.hasPrefix("orphans:") || freshIDs.contains(id) }
            forceEligible = forceEligible.intersection(freshIDs)
            scanStatuses["orphans"] = "Live process inspection"; moduleWarnings["orphans"] = []
            if selectedModuleID == "orphans" { progress = "Live process inspection"; warnings = [] }
        } catch {
            findings.removeAll { $0.moduleID == "orphans" }
            if !warnings.contains(error.localizedDescription), warnings.count < 200 { warnings.append(error.localizedDescription) }
            scanStatuses["orphans"] = "Process inspection incomplete"; moduleWarnings["orphans"] = [error.localizedDescription]
            if selectedModuleID == "orphans" { progress = "Process inspection incomplete" }
        }
    }
    public func restore(_ result: ActionResult) async {
        guard !demo, !isApplying else { return }
        isApplying = true
        defer { isApplying = false }
        do { try await core.restore(result); restoredIDs.insert(result.id) }
        catch { self.error = error.localizedDescription }
    }
    public func protect(_ finding: Finding) {
        if let path = finding.resource.path, !exclusions.contains(path) { exclusions.append(path); persist(); selectedIDs.remove(finding.id) }
    }
    public func ignore(_ finding: Finding) {
        guard case .process = finding.resource else { return }
        if !ignoredNames.contains(finding.title) { ignoredNames.append(finding.title); persist() }
        findings.removeAll { $0.id == finding.id }; selectedIDs.remove(finding.id)
    }
    public func reveal(_ finding: Finding) {
        if let folder = finding.details.first(where: { $0.label == "Working folder" || $0.label == "Data path" })?.value, FileManager.default.fileExists(atPath: folder) {
            NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: folder)
        } else if let path = finding.resource.path { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)]) }
    }
    private func loadDemo() {
        let base = "/Users/demo/Projects"
        func file(_ path: String) -> Resource { .file(FileIdentity(path: path, device: 1, inode: 1, modifiedSeconds: 0, modifiedNanos: 0)) }
        findings = [
            Finding(id: "demo-node", moduleID: "node", title: "dashboard", subtitle: base + "/dashboard/node_modules", resource: file(base + "/dashboard/node_modules"), bytes: 1_842_000_000, allocatedBytes: 1_210_000_000, details: [Detail("Project", base + "/dashboard"), Detail("Package manager", "pnpm"), Detail("Files", "34,126")], actions: [.trash], risk: .rebuild, reason: "Dependencies can be reinstalled. Review local patches before cleanup."),
            Finding(id: "demo-node-2", moduleID: "node", title: "landing-page", subtitle: base + "/landing-page/node_modules", resource: file(base + "/landing-page/node_modules"), bytes: 864_000_000, actions: [.trash], risk: .rebuild, reason: "Manifests and lockfiles are preserved."),
            Finding(id: "demo-storage", moduleID: "storage", title: "Projects", subtitle: base, resource: file(base), bytes: 14_280_000_000, reason: "Storage inventory. Inspect this folder to see its contents."),
            Finding(id: "demo-storage-2", moduleID: "storage", title: "Downloads", subtitle: "/Users/demo/Downloads", resource: file("/Users/demo/Downloads"), bytes: 3_510_000_000, reason: "Storage inventory."),
            Finding(id: "demo-simulator", moduleID: "simulators", title: "iPhone 16", subtitle: "iOS 18.5", resource: .simulator(id: "35D71E75-E2F0-4EF5-A131-06B14FDB044E", state: "Shutdown"), bytes: 4_320_000_000, details: [Detail("State", "Shutdown"), Detail("Identifier", "35D71E75-E2F0-4EF5-A131-06B14FDB044E")], actions: [.resetSimulator, .deleteSimulator], risk: .permanent, reason: "Device app data and settings are erased by reset or delete."),
            Finding(id: "demo-worktree", moduleID: "worktrees", title: "feature-navigation", subtitle: base + "/dashboard-worktrees/feature-navigation", resource: file(base + "/dashboard-worktrees/feature-navigation"), bytes: 2_540_000_000, details: [Detail("Branch", "feature/navigation"), Detail("State", "Local changes")], reason: "Local work needs inspection.", blockedReason: "Contains untracked or ignored files."),
            Finding(id: "demo-process", moduleID: "orphans", title: "node", subtitle: base + "/dashboard", resource: .process(ProcessIdentity(pid: 4201, uid: 501, startedSeconds: 1_791_399_000, startedMicroseconds: 1, executable: "/opt/homebrew/bin/node")), cpuPercent: 53.2, memoryBytes: 240_000_000, details: [Detail("PID", "4201"), Detail("Working folder", base + "/dashboard"), Detail("Command", "node dev-server.js")], actions: [.terminate, .forceQuit], risk: .permanent, reason: "No known managed job or running app owns this process. Review before terminating."),
        ]
        selectedModuleID = "node"; inspectedID = "demo-node"; progress = "Scan complete"; isScanning = false
    }
}

public enum Display {
    public static func items(_ count: Int) -> String { "\(count) " + (count == 1 ? "item" : "items") }
    public static func bytes(_ bytes: UInt64?) -> String {
        guard let bytes else { return "Unavailable" }
        return ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }
    public static func value(_ finding: Finding) -> String {
        if case .process = finding.resource {
            let cpu = finding.cpuPercent.map { String(format: "%.1f%% CPU", $0) } ?? "Sampling CPU"
            return cpu + " · " + bytes(finding.memoryBytes)
        }
        return bytes(finding.bytes)
    }
}
