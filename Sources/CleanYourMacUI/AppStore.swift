import AppKit
import CleanYourMacCore
import CleanYourMacDesignSystem
import Foundation
import Observation

public struct ReviewDraft: Identifiable {
    public let id = UUID()
    public let ids: [String]
    public let kind: ActionKind
    public let selection: SelectionSummary
    /// Protected items among `ids` the user confirmed removing.
    public var acknowledged: [String] = []
    /// Proceed even though running tools or apps use some items.
    public var force = false
}
/// Items an action skipped because something is using them, offered to force.
public struct InUseDraft: Identifiable {
    public let id = UUID()
    public let ids: [String]
    public let kind: ActionKind
    public let acknowledged: [String]
    public let message: String
}
public enum SortOrder: String, CaseIterable {
    case size = "Size", name = "Name", lastUsed = "Last used", cpu = "CPU"
    var core: ResultSort { switch self { case .size: .size; case .name: .name; case .lastUsed: .lastUsed; case .cpu: .cpu } }
}
public enum SizeFilter: String, CaseIterable {
    case all = "Any size", large = "100 MB+", huge = "1 GB+"
    var minimum: UInt64 { switch self { case .all: 0; case .large: 100_000_000; case .huge: 1_000_000_000 } }
}
/// How long ago something was modified or last used.
public enum AgeFilter: String, CaseIterable {
    case all = "Any time", day = "Over a day ago", week = "Over a week ago", month = "Over a month ago", year = "Over a year ago"
    var days: Double? { switch self { case .all: nil; case .day: 1; case .week: 7; case .month: 30; case .year: 365 } }
    /// The cutoff for the core's query: only items before this.
    var cutoff: Date? { days.map { Date().addingTimeInterval(-$0 * 86_400) } }
}

/// Capacity of the volume holding the home folder, as Finder reports it.
public struct DiskSpace: Equatable, Sendable {
    public let total: UInt64
    /// Free space right now.
    public let available: UInt64
    /// Free space including what macOS can purge on demand (caches, local snapshots).
    public let availableForImportantUsage: UInt64
    public init(total: UInt64, available: UInt64, availableForImportantUsage: UInt64) {
        self.total = total; self.available = available; self.availableForImportantUsage = availableForImportantUsage
    }
    public var used: UInt64 { total - min(total, available) }
    public var purgeable: UInt64 { availableForImportantUsage > available ? min(used, availableForImportantUsage - available) : 0 }

    public static func current(at path: String = NSHomeDirectory()) -> DiskSpace? {
        let keys: Set<URLResourceKey> = [.volumeTotalCapacityKey, .volumeAvailableCapacityKey, .volumeAvailableCapacityForImportantUsageKey]
        guard let values = try? URL(fileURLWithPath: path).resourceValues(forKeys: keys),
              let total = values.volumeTotalCapacity, let available = values.volumeAvailableCapacity else { return nil }
        let important = values.volumeAvailableCapacityForImportantUsage.map { UInt64(max(0, $0)) } ?? UInt64(available)
        return DiskSpace(total: UInt64(total), available: UInt64(available), availableForImportantUsage: important)
    }

    /// The disk split into what the last scan can free, the rest it found, purgeable space,
    /// everything else in use, and free space. Segments never exceed what is used.
    public func breakdown(found: UInt64, ready: UInt64) -> [DiskBar.Segment] {
        let readyBytes = min(ready, used)
        let foundBytes = min(found, used)
        // "Found" includes what is ready to free; show the rest of it separately.
        let otherFound = foundBytes - min(readyBytes, foundBytes)
        let purgeableBytes = min(purgeable, used - readyBytes - otherFound)
        let otherUsed = used - readyBytes - otherFound - purgeableBytes
        return [
            .init("Ready to free", bytes: readyBytes, style: .ready),
            .init("Other found", bytes: otherFound, style: .found),
            .init("Purgeable", bytes: purgeableBytes, style: .purgeable),
            .init("Other used", bytes: otherUsed, style: .used),
            .init("Free", bytes: available, style: .free),
        ]
    }
}

/// App state. Findings live in the core's result store; this store keeps only the current
/// query's totals and a bounded cache of row pages, so result sets of millions stay smooth.
@MainActor @Observable
public final class AppStore {
    public static let pageSize = 200
    private static let maxPages = 60

    public let core: CoreEngine
    public let modules: [ModuleDescriptor]
    public var selectedModuleID: String? = "overview" {
        didSet {
            if selectedModuleID != oldValue {
                selectedIDs = []; inspectedID = nil; search = ""; sizeFilter = .all; ageFilter = .all; usedFilter = .all
                if !isScanning && !demo {
                    progress = scanStatuses[selectedModuleID ?? "overview"] ?? "Ready · scan this tool"
                    warnings = moduleWarnings[selectedModuleID ?? "overview"] ?? []
                }
            }
        }
    }
    /// Where a scan looks: the whole Mac (your home folder) or the folders you chose.
    public enum ScanPlace: String, CaseIterable, Sendable {
        case wholeMac = "Whole Mac", folders = "Chosen folders"
    }
    public var scanPlace: ScanPlace = .wholeMac { didSet { if scanPlace != oldValue { storageNavigation = []; persist(); scopeChanged() } } }
    /// The folders a scan starts from. "Whole Mac" means your home folder: system locations
    /// are never scanned, and other volumes are left alone.
    public var scanRoots: [String] { scanPlace == .wholeMac && !demo ? [NSHomeDirectory()] : roots }
    /// When the last full scan from the overview finished.
    public private(set) var lastScanAt: Date?
    /// One-click fixes for the current results, largest first.
    public private(set) var recommendations: [Recommendation] = []
    /// The home volume's capacity, refreshed with the overview.
    public private(set) var diskSpace = DiskSpace.current()
    public var roots: [String] { didSet { if roots != oldValue { scopeChanged() } } }
    public var exclusions: [String] { didSet { if exclusions != oldValue { scopeChanged() } } }
    public var ignoredNames: [String]
    public var disabledModules: [String]
    public var selectedIDs = Set<String>()
    public var inspectedID: String? { didSet { if inspectedID != oldValue { loadInspected() } } }
    /// The full finding behind `inspectedID`, fetched from the core on demand.
    public private(set) var inspected: Finding?
    public var search = ""
    /// Size and CPU start largest first, Name starts A–Z and Last used starts with the least
    /// recently used; the direction can then be flipped.
    public var sort: SortOrder = .size { didSet { if sort != oldValue { sortAscending = sort == .name || sort == .lastUsed } } }
    public var sortAscending = false
    public var sizeFilter: SizeFilter = .all
    /// Modified longer ago than this.
    public var ageFilter: AgeFilter = .all
    /// Last used longer ago than this; items with no recorded use count by modification.
    public var usedFilter: AgeFilter = .all
    public var isScanning = false
    public var isApplying = false
    public var progress = "Ready"
    public var warnings: [String] = []
    public var history: [ActionResult] = []
    public var review: ReviewDraft?
    /// Items the last action left alone because they are in use; the app offers to force them.
    public var inUse: InUseDraft?
    public var error: String?
    public var showInspector = true
    public var menuBarEnabled = false { didSet { persist(); configureMonitor() } }
    /// The Comfy palette; colors resolve against it at draw time.
    public var theme: ComfyTheme = ComfyTheme.current { didSet { ComfyTheme.current = theme; persist() } }
    /// Light, dark, or following the system; every palette has both appearances.
    public enum Appearance: String, CaseIterable, Sendable {
        case system = "System", light = "Light", dark = "Dark"
        @MainActor func apply() {
            NSApplication.shared.appearance = switch self {
            case .system: nil
            case .light: NSAppearance(named: .aqua)
            case .dark: NSAppearance(named: .darkAqua)
            }
        }
    }
    public var appearance: Appearance = .system { didSet { appearance.apply(); persist() } }
    public var forceEligible = Set<String>()
    public var restoredIDs = Set<UUID>()
    public var storageNavigation: [String] = []

    // Current query: totals and a page cache. Rows are fetched only for what is on screen.
    public private(set) var queryID: UInt64 = 0
    public private(set) var resultTotal = 0
    /// Rows in the current results an action can apply to.
    public private(set) var eligibleTotal = 0
    /// Core-computed totals for the rows matching the search and filters.
    public private(set) var summary = Analytics.empty
    public private(set) var largest: [CleanYourMacCore.ResultRow] = []
    public private(set) var maxCPU: Double?
    /// Totals across every module, for the overview, sidebar counts and menu bar.
    public private(set) var overview = Analytics.empty
    public private(set) var selection = SelectionSummary.empty
    private var pages: [Int: [CleanYourMacCore.ResultRow]] = [:]
    private var loadingPages = Set<Int>()
    /// Bumped whenever stored results change, so the visible query refreshes.
    public private(set) var storeVersion = 0

    private var scanStatuses: [String: String] = [:]
    private var moduleWarnings: [String: [String]] = [:]
    private var scanningModules: [String] = []
    /// Modules the running scan covers, and how many have finished.
    public private(set) var scanTotal = 0
    public private(set) var scanFinished = 0
    /// Modules of the running scan whose results are complete, and so can be acted on.
    public private(set) var finishedModuleIDs = Set<String>()
    /// Whether a fix's results are final: no scan is running, or its module has finished.
    public func canReview(_ fix: Recommendation) -> Bool {
        !isApplying && (!isScanning || finishedModuleIDs.contains(fix.moduleID))
    }
    /// The running scan's completed share, when it covers more than one module.
    public var scanFraction: Double? { scanTotal > 1 ? min(1, Double(scanFinished) / Double(scanTotal)) : nil }
    public let demo: Bool
    private var scanTask: Task<Void, Never>?
    private var scanID: UUID?
    private var scanHadWarnings = false
    private var monitorTask: Task<Void, Never>?
    private var isRefreshingOrphans = false
    private var queryToken = 0
    private var demoLoad: Task<Void, Never>?
    private let defaults: UserDefaults

    public init(demo: Bool = false, demoRows: Int = 0, defaults: UserDefaults = .standard, core: CoreEngine = .shared) {
        self.defaults = defaults; self.demo = demo; self.core = core
        modules = core.modules()
        let initial = core.projectRoots()
        roots = core.normalizeRoots(defaults.stringArray(forKey: "scanRoots") ?? (initial.isEmpty ? [NSHomeDirectory() + "/Downloads"] : initial))
        exclusions = defaults.stringArray(forKey: "excludedPaths") ?? []
        ignoredNames = defaults.stringArray(forKey: "ignoredProcessNames") ?? ["ssh-agent", "gpg-agent", "keyboxd", "dirmngr"]
        disabledModules = defaults.stringArray(forKey: "disabledModules") ?? []
        menuBarEnabled = !demo && defaults.bool(forKey: "menuBarEnabled")
        theme = defaults.string(forKey: "theme").flatMap(ComfyTheme.init(rawValue:)) ?? ComfyTheme.standard
        // Paper & Walnut was the default before White & Gray; move people who kept it, once.
        if defaults.integer(forKey: "themeDefaultsVersion") < 2 {
            if theme == .walnut { theme = ComfyTheme.standard }
            defaults.set(2, forKey: "themeDefaultsVersion")
        }
        appearance = defaults.string(forKey: "appearance").flatMap(Appearance.init(rawValue:)) ?? .system
        scanPlace = defaults.string(forKey: "scanPlace").flatMap(ScanPlace.init(rawValue:)) ?? .wholeMac
        lastScanAt = defaults.object(forKey: "lastScanAt") as? Date
        ComfyTheme.current = theme
        configureMonitor()
        if demo { roots = ["/Users/demo/Projects"]; exclusions = []; disabledModules = []; loadDemo(rows: demoRows) }
    }
    /// With chosen folders, every tool is limited to them, including those that read fixed
    /// locations such as caches, toolchains and apps.
    public var context: ScanContext {
        ScanContext(
            roots: selectedModuleID == "storage" && !storageNavigation.isEmpty ? [storageNavigation.last!] : scanRoots,
            exclusions: exclusions, ignoredProcessNames: ignoredNames,
            limitToRoots: scanPlace == .folders && !demo
        )
    }
    public var enabledModules: [ModuleDescriptor] { modules.filter { !disabledModules.contains($0.id) } }
    public var currentModule: ModuleDescriptor? { modules.first { $0.id == selectedModuleID } }
    public func count(for module: String) -> Int { overview.modules.first { $0.moduleId == module }?.count ?? 0 }

    // MARK: Querying

    public var currentQuery: ResultQuery {
        ResultQuery(
            module: selectedModuleID == "overview" ? nil : selectedModuleID,
            search: search,
            minBytes: sizeFilter.minimum,
            modifiedBefore: ageFilter.cutoff,
            lastUsedBefore: usedFilter.cutoff,
            sort: sort.core,
            ascending: sortAscending
        )
    }
    /// Changes whenever the visible rows can change; drives debounced re-queries.
    public var queryKey: String {
        "\(selectedModuleID ?? "")|\(search)|\(sizeFilter.rawValue)|\(ageFilter.rawValue)|\(usedFilter.rawValue)|\(sort.rawValue)|\(sortAscending)|\(storeVersion)|\(storageNavigation.count)"
    }
    /// Runs the current query in the core and resets the page cache. Stale answers are dropped.
    public func requery() async {
        if let demoLoad { await demoLoad.value }
        queryToken += 1
        let token = queryToken
        guard let info = try? await core.query(currentQuery), token == queryToken else { return }
        queryID = info.queryId; resultTotal = info.total; summary = info.summary
        largest = info.largest; maxCPU = info.maxCpu; eligibleTotal = info.eligible
        pages = [:]; loadingPages = []
        if currentQuery.module == nil { overview = info.summary }
    }
    public func refreshOverview() async {
        if let info = try? await core.query(ResultQuery(module: nil)) { overview = info.summary }
        recommendations = await core.recommendations()
        diskSpace = DiskSpace.current()
    }
    // MARK: Why controls are unavailable, shown on hover

    public var whileApplying: String? { isApplying ? "Wait for the current action to finish." : nil }
    public var whileScanning: String? { isScanning ? "Available when the scan finishes." : nil }
    public var inDemo: String? { demo ? "Turned off in the demo; nothing on your Mac is changed." : nil }
    /// The first reason that applies.
    public func reason(_ reasons: String?...) -> String? { reasons.lazy.compactMap { $0 }.first }
    /// Why a row's checkbox cannot be ticked.
    public func notSelectable(_ row: CleanYourMacCore.ResultRow) -> String? {
        if let busy = whileApplying { return busy }
        guard !row.eligible else { return nil }
        if row.acknowledgeable { return "Protected. Open it in the inspector to move it to Trash anyway." }
        return row.blockedReason ?? "Listed for information; nothing can be done with it here."
    }
    /// Repeats an action on items that were in use, after the user chose to force it.
    public func forceInUse() async {
        guard let draft = inUse, !isApplying else { return }
        inUse = nil
        await apply(ReviewDraft(ids: draft.ids, kind: draft.kind, selection: .empty, acknowledged: draft.acknowledged, force: true))
    }
    /// Moves one protected item the user explicitly confirmed losing.
    public func applyAcknowledged(_ finding: Finding, kind: ActionKind) async {
        guard !isApplying, !isScanning, finding.acknowledgedActions?.contains(kind) == true else { return }
        let summary = await core.selection([finding.id], preview: 0)
        await apply(ReviewDraft(ids: [finding.id], kind: kind, selection: summary, acknowledged: [finding.id]))
    }

    /// After the folders to scan or the exclusions change: forget results outside the new
    /// scope, with their selection, so nothing listed or recommended lies outside it.
    private func scopeChanged() {
        guard !demo else { return }
        let context = ScanContext(roots: scanRoots, exclusions: exclusions, ignoredProcessNames: ignoredNames, limitToRoots: scanPlace == .folders)
        Task { [weak self] in
            guard let self else { return }
            let removed = Set(await self.core.retainInScope(context))
            guard !removed.isEmpty else { return }
            self.selectedIDs.subtract(removed)
            if let id = self.inspectedID, removed.contains(id) { self.inspectedID = nil }
            self.resultsChanged()
            await self.refreshOverview()
        }
    }
    /// When the app becomes active again: forget results deleted elsewhere in the meantime
    /// (in Finder, a terminal or another app) and refresh totals, fixes and disk space.
    public func refreshOnReturn() async {
        guard !demo, !isScanning, !isApplying else { return }
        let removed = Set(await core.pruneMissing())
        if !removed.isEmpty {
            selectedIDs.subtract(removed)
            if let id = inspectedID, removed.contains(id) { inspectedID = nil }
            resultsChanged()
        }
        await refreshOverview()
    }
    /// Bytes the recommended Move to Trash fixes free together.
    public var recommendedBytes: UInt64 { recommendations.filter { $0.action == .trash }.reduce(0) { $0 + $1.bytes } }
    /// Opens the review for one recommended fix, with its items selected.
    public func review(_ fix: Recommendation) {
        guard canReview(fix) else { return }
        selectedIDs = Set(fix.ids)
        reviewSelection(fix.action)
    }
    /// Opens one review for every recommended Move to Trash fix.
    public func reviewAllRecommendations() {
        guard !isApplying, !isScanning else { return }
        selectedIDs = Set(recommendations.filter { $0.action == .trash }.flatMap(\.ids))
        if !selectedIDs.isEmpty { reviewSelection(.trash) }
    }
    /// The row at `index` if its page is cached. Call `prefetch` to load it.
    public func row(at index: Int) -> CleanYourMacCore.ResultRow? {
        let page = pages[index / Self.pageSize]
        let offset = index % Self.pageSize
        return page.flatMap { offset < $0.count ? $0[offset] : nil }
    }
    /// Loads the page containing `index` (and the next one near a page end).
    public func prefetch(_ index: Int) {
        let page = index / Self.pageSize
        load(page)
        if index % Self.pageSize > Self.pageSize * 3 / 4 { load(page + 1) }
    }
    private func load(_ page: Int) {
        guard page >= 0, page * Self.pageSize < resultTotal, pages[page] == nil, !loadingPages.contains(page) else { return }
        loadingPages.insert(page)
        let query = queryID
        Task { [weak self] in
            guard let self else { return }
            let rows = (try? await self.core.rows(queryID: query, offset: page * Self.pageSize, limit: Self.pageSize)) ?? []
            guard query == self.queryID else { return }
            self.loadingPages.remove(page)
            self.pages[page] = rows
            if self.pages.count > Self.maxPages {
                // Keep the pages nearest the one just loaded.
                for far in self.pages.keys.sorted(by: { abs($0 - page) > abs($1 - page) }).prefix(self.pages.count - Self.maxPages) {
                    self.pages[far] = nil
                }
            }
        }
    }
    /// Rows in `range`, loading them directly; for tests and previews.
    public func rows(_ range: Range<Int>) async -> [CleanYourMacCore.ResultRow] {
        (try? await core.rows(queryID: queryID, offset: range.lowerBound, limit: range.count)) ?? []
    }
    private func loadInspected() {
        guard let id = inspectedID else { inspected = nil; return }
        Task { [weak self] in
            if let demoLoad = self?.demoLoad { await demoLoad.value }
            let finding = await self?.core.finding(id)
            if self?.inspectedID == id { self?.inspected = finding }
        }
    }
    private func resultsChanged() { storeVersion += 1 }

    // MARK: Selection

    public var availableActions: [ActionKind] {
        selection.actions.filter { $0 != .forceQuit || selectedIDs.isSubset(of: forceEligible) }
    }
    public func select(_ id: String, checked: Bool) {
        if checked { selectedIDs.insert(id) } else { selectedIDs.remove(id) }
    }
    /// How a row was clicked: plainly, with Command, or with Shift.
    public enum Click { case plain, toggle, extend }
    /// Where a Shift range starts, and the row last clicked or moved to, in one query.
    @ObservationIgnored private var anchor: (query: UInt64, index: Int)?
    @ObservationIgnored private var cursor: (query: UInt64, index: Int)?
    /// The row last clicked or moved to, if it belongs to the current query.
    public var cursorIndex: Int? { cursor.flatMap { $0.query == queryID ? $0.index : nil } }
    /// A plain click inspects a row; Command-click also adds it to or removes it from the
    /// selection; Shift-click selects every eligible row from the last plain or Command
    /// click to this one.
    public func click(_ row: ResultRow, at index: Int, _ kind: Click) {
        inspectedID = row.id
        cursor = (queryID, index)
        switch kind {
        case .plain:
            anchor = (queryID, index)
        case .toggle:
            if row.eligible, !isApplying { select(row.id, checked: !selectedIDs.contains(row.id)) }
            anchor = (queryID, index)
        case .extend:
            guard let start = anchor, start.query == queryID else {
                if row.eligible, !isApplying { select(row.id, checked: true) }
                anchor = (queryID, index)
                return
            }
            let range = min(start.index, index)..<(max(start.index, index) + 1)
            Task { await selectEligible(in: range) }
        }
    }
    /// Selects every eligible row of the current results.
    public func selectAllResults() {
        let total = resultTotal
        Task { await selectEligible(in: 0..<total) }
    }
    public func deselectAll() { selectedIDs = [] }
    /// Whether nothing, some or every selectable row of the current results is selected.
    public enum SelectionState { case empty, partial, all }
    public var selectionState: SelectionState {
        if selectedIDs.isEmpty || eligibleTotal == 0 { return .empty }
        return selectedIDs.count >= eligibleTotal ? .all : .partial
    }
    /// The header checkbox: selects every eligible row, or clears a complete selection.
    public func toggleSelectAll() {
        if selectionState == .all { deselectAll() } else { selectAllResults() }
    }
    /// Space: toggles the row last clicked or moved to.
    public func toggleCursorRow() {
        guard let index = cursorIndex, let row = row(at: index) else { return }
        click(row, at: index, .toggle)
    }
    /// Arrow keys: inspects the next or previous row; with Shift, also selects from the
    /// anchor to it. Returns the new row's index so the table can scroll to it.
    public func moveCursor(by delta: Int, extend: Bool) async -> Int? {
        guard resultTotal > 0 else { return nil }
        let target = max(0, min(resultTotal - 1, (cursorIndex ?? -1) + delta))
        let query = queryID
        let found: ResultRow?
        if let cached = row(at: target) { found = cached } else { found = await rows(target..<(target + 1)).first }
        guard query == queryID, let row = found else { return nil }
        click(row, at: target, extend ? .extend : .plain)
        return target
    }
    /// Adds the eligible rows in `range` of the current query, fetched as IDs in large pages.
    private func selectEligible(in range: Range<Int>) async {
        guard !isApplying else { return }
        let query = queryID
        var offset = range.lowerBound
        while offset < range.upperBound {
            let limit = min(100_000, range.upperBound - offset)
            guard let ids = try? await core.eligibleIDs(queryID: query, offset: offset, limit: limit), query == queryID else { return }
            selectedIDs.formUnion(ids)
            offset += limit
        }
    }
    public func refreshSelection() async {
        let ids = Array(selectedIDs)
        let summary = ids.isEmpty ? .empty : await core.selection(ids, preview: 0)
        if Set(ids) == selectedIDs { selection = summary }
    }

    // MARK: Settings

    public func persist() {
        guard !demo else { return }
        defaults.set(roots, forKey: "scanRoots"); defaults.set(exclusions, forKey: "excludedPaths")
        defaults.set(ignoredNames, forKey: "ignoredProcessNames"); defaults.set(disabledModules, forKey: "disabledModules")
        defaults.set(menuBarEnabled, forKey: "menuBarEnabled"); defaults.set(theme.rawValue, forKey: "theme")
        defaults.set(appearance.rawValue, forKey: "appearance")
        defaults.set(scanPlace.rawValue, forKey: "scanPlace"); defaults.set(lastScanAt, forKey: "lastScanAt")
    }
    /// Scans the folders the user picks from now on, instead of the previous ones.
    /// Returns whether folders were chosen.
    @discardableResult public func chooseFolders() -> Bool { pickFolders(replacing: true) }
    /// Adds folders to the chosen ones, as Settings does.
    public func addRoot() { pickFolders(replacing: false) }
    @discardableResult private func pickFolders(replacing: Bool) -> Bool {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false; panel.canChooseDirectories = true; panel.allowsMultipleSelection = true
        panel.prompt = "Scan"
        panel.message = "Choose folders to inspect. Scanning never removes files."
        if panel.runModal() == .OK, !panel.urls.isEmpty {
            let chosen = panel.urls.map(\.path)
            roots = core.normalizeRoots(replacing ? chosen : roots + chosen)
            storageNavigation = []
            scanPlace = .folders
            persist()
            return true
        }
        // Nothing to scan without a folder.
        if scanPlace == .folders && roots.isEmpty { scanPlace = .wholeMac }
        return false
    }

    // MARK: Scanning

    public func scan() {
        guard !isApplying else { return }
        if demo { loadDemo(rows: 0); return }
        if scanPlace == .folders && roots.isEmpty && !chooseFolders() { return }
        if isScanning { cancelScan() }
        scanTask?.cancel()
        // An overview scan rescans Storage Explorer's top level, so leave any opened folder.
        if (selectedModuleID ?? "overview") == "overview" { storageNavigation = [] }
        let id = UUID(), selected = selectedModuleID ?? "overview", context = self.context
        scanID = id; isScanning = true; progress = "Starting scan…"; warnings = []; scanHadWarnings = false; selectedIDs = []; inspectedID = nil
        // The overview scans every enabled module except those the user must run explicitly.
        let ids = selected == "overview" ? enabledModules.filter(\.inOverview).map(\.id) : [selected]
        scanningModules = Array(Set(ids + [selected]))
        scanTotal = ids.count; scanFinished = 0; finishedModuleIDs = []
        let stream = core.scan(moduleIDs: ids, context: context, store: true)
        scanTask = Task { [weak self] in
            guard let self else { return }
            do {
                for try await event in stream {
                    guard self.scanID == id else { return }
                    try Task.checkCancellation()
                    switch event {
                    case .stored: self.resultsChanged()
                    case .progress(let message): self.progress = message
                    case .warning(let message):
                        self.scanHadWarnings = true
                        if self.warnings.count < 200 { self.warnings.append(message) }
                    case .moduleFinished(let module):
                        self.scanFinished += 1
                        self.finishedModuleIDs.insert(module)
                    case .finding: break
                    }
                }
                if self.scanID == id {
                    if selected == "overview" { self.lastScanAt = Date(); self.persist() }
                    self.finishScanStatus(self.scanHadWarnings ? "Partial scan · review warnings" : "Scan complete")
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
        resultsChanged()
        Task { await refreshOverview() }
    }
    public func cancelScan() { scanTask?.cancel(); scanID = nil; finishScanStatus("Scan cancelled · partial results") }
    public func browse(_ path: String) {
        guard !demo, scanRoots.contains(where: { core.contains(path, in: $0) }), !exclusions.contains(where: { core.contains(path, in: $0) }) else { return }
        storageNavigation.append(path); scan()
    }
    public func browseBack() {
        guard !storageNavigation.isEmpty else { return }
        storageNavigation.removeLast(); scan()
    }

    // MARK: Actions

    public func reviewSelection(_ kind: ActionKind) {
        let ids = Array(selectedIDs)
        Task { [weak self] in
            guard let self else { return }
            let summary = await self.core.selection(ids, preview: 200)
            self.review = ReviewDraft(ids: ids, kind: kind, selection: summary)
        }
    }
    public func apply(_ draft: ReviewDraft) async {
        guard !demo else { error = "Demo mode cannot modify files or processes."; return }
        isApplying = true
        let results: [ActionResult]
        do { results = try await core.executeSelection(draft.ids, kind: draft.kind, context: context, acknowledged: draft.acknowledged, force: draft.force) }
        catch { self.error = error.localizedDescription; isApplying = false; return }
        for result in results {
            guard let id = result.findingID else { continue }
            if result.outcome == .requested && result.action == .terminate { forceEligible.insert(id) }
            if result.outcome == .applied || result.outcome == .skipped { selectedIDs.remove(id); forceEligible.remove(id) }
        }
        mergeHistory(results + (await core.history()))
        isApplying = false; review = nil
        resultsChanged()
        await refreshOverview()
        // The core attempts every item, so one failure never stops the rest. Items skipped only
        // because something uses them can be forced; other failures are final and listed.
        let busy = draft.force ? [] : results.filter { $0.outcome == .failed && $0.overridable == true }
        let busyIDs = Set(busy.map(\.id))
        let failed = results.filter { $0.outcome == .failed && !busyIDs.contains($0.id) }
        let finished = Self.batchSummary(results, failed: failed.count + busy.count)
        let failures = failed.isEmpty ? "" : (failed.count == 1 ? "1 item could not be changed:" : "\(failed.count) items could not be changed:")
            + "\n" + Self.listed(failed.map { "\($0.title): \($0.message)" })
        if !busy.isEmpty {
            let count = busy.count == 1 ? "This item is" : "\(busy.count) items are"
            inUse = InUseDraft(
                ids: busy.compactMap(\.findingID), kind: draft.kind, acknowledged: draft.acknowledged,
                message: [finished, "\(count) in use.\n\n" + Self.unique(busy.map(\.message)).joined(separator: "\n\n"), failures]
                    .filter { !$0.isEmpty }.joined(separator: "\n\n")
                    + (draft.kind == .trash ? "\n\nForcing it can disrupt the programs using it. Items moved to the Trash can be restored from Activity." : "")
            )
        } else if !failed.isEmpty {
            error = results.count == 1 ? failed[0].message : [finished, failures].filter { !$0.isEmpty }.joined(separator: "\n\n")
        } else if let warning = results.compactMap(\.journalWarning).first { error = warning }
    }
    /// "38 of 40 items done." for a batch where some items failed; empty otherwise.
    private static func batchSummary(_ results: [ActionResult], failed: Int) -> String {
        let done = results.filter { $0.outcome == .applied || $0.outcome == .skipped }.count
        guard failed > 0, results.count > 1 else { return "" }
        return "\(done) of \(results.count) items done. Every item was attempted; the others were left as they were."
    }
    private static func unique(_ messages: [String]) -> [String] {
        var seen = Set<String>()
        return messages.filter { seen.insert($0).inserted }
    }
    /// Up to eight lines, then a count of the rest.
    private static func listed(_ lines: [String]) -> String {
        let shown = lines.prefix(8).map { "• " + $0 }
        return (shown + (lines.count > 8 ? ["and \(lines.count - 8) more, listed in Activity."] : [])).joined(separator: "\n")
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
            // The whole scan scope, not a folder opened in Storage Explorer.
            let scope = ScanContext(roots: scanRoots, exclusions: exclusions, ignoredProcessNames: ignoredNames, limitToRoots: scanPlace == .folders && !demo)
            for try await _ in core.scan(moduleIDs: ["orphans"], context: scope, store: true) {}
            let orphanIDs = selectedIDs.filter { $0.hasPrefix("orphans:") }
            if !orphanIDs.isEmpty {
                let present = Set(await core.selection(Array(orphanIDs), preview: orphanIDs.count).preview.map(\.id))
                selectedIDs.subtract(orphanIDs.subtracting(present))
                forceEligible = forceEligible.filter { !$0.hasPrefix("orphans:") || present.contains($0) }
            }
            scanStatuses["orphans"] = "Live process inspection"; moduleWarnings["orphans"] = []
            if selectedModuleID == "orphans" { progress = "Live process inspection"; warnings = [] }
        } catch {
            if !warnings.contains(error.localizedDescription), warnings.count < 200 { warnings.append(error.localizedDescription) }
            scanStatuses["orphans"] = "Process inspection incomplete"; moduleWarnings["orphans"] = [error.localizedDescription]
            if selectedModuleID == "orphans" { progress = "Process inspection incomplete" }
        }
        resultsChanged()
    }
    public func restore(_ result: ActionResult) async {
        guard !demo, !isApplying else { return }
        isApplying = true
        defer { isApplying = false }
        do { try await core.restore(result); restoredIDs.insert(result.id) }
        catch { self.error = error.localizedDescription }
    }
    public func protect(path: String?, id: String) {
        if let path, !exclusions.contains(path) { exclusions.append(path); persist(); selectedIDs.remove(id) }
    }
    public func ignore(processName: String, id: String) {
        if !ignoredNames.contains(processName) { ignoredNames.append(processName); persist() }
        selectedIDs.remove(id)
        Task { [weak self] in
            await self?.core.removeResults([id])
            self?.resultsChanged()
        }
    }
    public func reveal(id: String, path: String?) {
        Task { [weak self] in
            let finding = await self?.core.finding(id)
            if let folder = finding?.value("Working folder") ?? finding?.value("Data path"), FileManager.default.fileExists(atPath: folder) {
                NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: folder)
            } else if let path = finding?.resource.path ?? path {
                NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
            }
        }
    }

    // MARK: Demo

    private func loadDemo(rows: Int) {
        let base = "/Users/demo/Projects"
        func file(_ path: String) -> Resource { .file(FileIdentity(path: path, device: 1, inode: 1, modifiedSeconds: 0, modifiedNanos: 0)) }
        let fixtures = [
            Finding(id: "demo-node", moduleID: "node", title: "dashboard", subtitle: base + "/dashboard/node_modules", resource: file(base + "/dashboard/node_modules"), bytes: 1_842_000_000, allocatedBytes: 1_210_000_000, details: [Detail("Project", base + "/dashboard"), Detail("Package manager", "pnpm"), Detail("Files", "34,126")], actions: [.trash], risk: .rebuild, reason: "Dependencies can be reinstalled. Review local patches before cleanup."),
            Finding(id: "demo-node-2", moduleID: "node", title: "landing-page", subtitle: base + "/landing-page/node_modules", resource: file(base + "/landing-page/node_modules"), bytes: 864_000_000, actions: [.trash], risk: .rebuild, reason: "Manifests and lockfiles are preserved."),
            Finding(id: "demo-storage", moduleID: "storage", title: "Projects", subtitle: base, resource: file(base), bytes: 14_280_000_000, reason: "Storage inventory. Inspect this folder to see its contents."),
            Finding(id: "demo-storage-2", moduleID: "storage", title: "Downloads", subtitle: "/Users/demo/Downloads", resource: file("/Users/demo/Downloads"), bytes: 3_510_000_000, reason: "Storage inventory."),
            Finding(id: "demo-simulator", moduleID: "simulators", title: "iPhone 16", subtitle: "iOS 18.5", resource: .simulator(id: "35D71E75-E2F0-4EF5-A131-06B14FDB044E", state: "Shutdown"), bytes: 4_320_000_000, details: [Detail("State", "Shutdown"), Detail("Identifier", "35D71E75-E2F0-4EF5-A131-06B14FDB044E"), Detail("Data path", "/Users/demo/Library/Developer/CoreSimulator/Devices/35D71E75-E2F0-4EF5-A131-06B14FDB044E/data")], actions: [.resetSimulator, .deleteSimulator], risk: .permanent, reason: "Device app data and settings are erased by reset or delete."),
            Finding(id: "demo-worktree", moduleID: "worktrees", title: "feature-navigation", subtitle: "feature/navigation · " + base + "/dashboard-worktrees/feature-navigation", resource: file(base + "/dashboard-worktrees/feature-navigation"), bytes: 2_540_000_000, details: [Detail("Branch", "feature/navigation"), Detail("State", "Local changes")], reason: "Local work needs inspection.", blockedReason: "Contains untracked files.", badge: "Linked"),
            Finding(id: "demo-process", moduleID: "orphans", title: "node", subtitle: base + "/dashboard", resource: .process(ProcessIdentity(pid: 4201, uid: 501, startedSeconds: 1_791_399_000, startedMicroseconds: 1, executable: "/opt/homebrew/bin/node")), cpuPercent: 53.2, memoryBytes: 240_000_000, details: [Detail("PID", "4201"), Detail("Working folder", base + "/dashboard"), Detail("Command", "node dev-server.js")], actions: [.terminate, .forceQuit], risk: .permanent, reason: "No known managed job or running app owns this process. Review before terminating."),
        ]
        demoLoad = Task { [core] in
            if rows > 0 { _ = await core.synthesize(rows) } else { await core.loadResults(fixtures) }
        }
        selectedModuleID = rows > 0 ? "large" : "node"
        inspectedID = rows > 0 ? nil : "demo-node"
        progress = "Scan complete"; isScanning = false
        Task { [weak self] in
            await self?.requery()
            await self?.refreshOverview()
        }
    }
}

public enum Display {
    /// Modules whose findings are plain files, folders or apps, shown with their Finder icons.
    static let fileModules: Set<String> = ["large", "duplicates", "downloads", "trash", "storage", "applications", "leftovers"]
    /// A finding's leading icon: its ecosystem's logo, its Finder icon, or the module's symbol.
    @MainActor public static func icon(moduleID: String, path: String?, brand: String?, symbol: String) -> RowIcon {
        if let brand, BrandCatalog.contains(brand) { return .brand(brand) }
        if fileModules.contains(moduleID), let path { return .file(FileIcons.icon(for: path)) }
        return .symbol(symbol)
    }
    public static func items(_ count: Int) -> String { "\(count.formatted()) " + (count == 1 ? "item" : "items") }
    public static func bytes(_ bytes: UInt64?) -> String {
        guard let bytes else { return "Unavailable" }
        return ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    }
    /// "used 3 months ago", for row badges.
    public static func lastUsed(_ date: Date?) -> String? {
        date.map { "used " + $0.formatted(.relative(presentation: .named)) }
    }
    public static func value(_ row: CleanYourMacCore.ResultRow) -> String {
        if row.isProcess {
            let cpu = row.cpuPercent.map { String(format: "%.1f%% CPU", $0) } ?? "Sampling CPU"
            return cpu + " · " + bytes(row.memoryBytes)
        }
        return bytes(row.diskBytes)
    }
}
