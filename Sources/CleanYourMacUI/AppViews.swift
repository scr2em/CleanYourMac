import AppKit
import CleanYourMacCore
import CleanYourMacDesignSystem
import SwiftUI

public struct WorkspaceView: View {
    @Bindable private var store: AppStore
    public init(store: AppStore) { self.store = store }
    public var body: some View {
        NavigationSplitView {
            SidebarView(store: store)
                .navigationSplitViewColumnWidth(ideal: Layout.sidebar)
        } detail: {
            HSplitView {
                content.frame(minWidth: Layout.contentMin)
                if store.showInspector, store.inspected != nil, store.selectedModuleID != "activity", store.selectedModuleID != "gallery" {
                    InspectorView(store: store).frame(minWidth: Layout.inspectorMin, idealWidth: Layout.inspector, maxWidth: Layout.inspectorMax)
                }
            }
        }
        .frame(minWidth: Layout.windowMinWidth, minHeight: Layout.windowMinHeight)
        .background(Palette.canvas)
        .id(store.theme)
        .toolbar {
            ToolbarItemGroup {
                if store.demo { Text("Demo · actions disabled").font(TypeStyle.caption).foregroundStyle(.secondary) }
                Button { store.showInspector.toggle() } label: { Label("Inspector", systemImage: "sidebar.right") }.help("Toggle inspector")
                SettingsLink { Label("Settings", systemImage: "gearshape") }.help("Settings")
            }
        }
        .sheet(item: $store.review) { draft in ReviewView(store: store, draft: draft) }
        .alert("Attention needed", isPresented: Binding(get: { store.error != nil }, set: { if !$0 { store.error = nil } })) {
            Button("OK") { store.error = nil }
        } message: { Text(store.error ?? "") }
        .alert("In use", isPresented: Binding(get: { store.inUse != nil && store.error == nil }, set: { if !$0 { store.inUse = nil } })) {
            Button("\(store.inUse?.kind.label ?? "Continue") Anyway", role: .destructive) { Task { await store.forceInUse() } }
            Button("Cancel", role: .cancel) { store.inUse = nil }
        } message: { Text(store.inUse?.message ?? "") }
        .task { if !store.demo { await store.loadHistory() }; await store.refreshOverview() }
        // Things listed may have been deleted while the app was in the background.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await store.refreshOnReturn() }
        }
        .task(id: store.selectedModuleID) {
            if store.selectedModuleID == "orphans" {
                while !Task.isCancelled {
                    await store.refreshOrphans()
                    try? await Task.sleep(for: .seconds(2))
                }
            }
        }
    }
    @ViewBuilder private var content: some View {
        switch store.selectedModuleID {
        case "overview": OverviewView(store: store)
        case "activity": ActivityView(store: store)
        case "gallery": ComponentGallery()
        default: FinderView(store: store)
        }
    }
}

private struct SidebarView: View {
    @Bindable var store: AppStore
    var body: some View {
        List(selection: $store.selectedModuleID) {
            Label("Overview", systemImage: "square.grid.2x2").tag("overview")
            ForEach(Category.allCases, id: \.self) { category in
                Section(category.rawValue) {
                    ForEach(store.enabledModules.filter { $0.category == category }, id: \.id) { module in
                        Label(module.name, systemImage: module.symbol).tag(module.id)
                    }
                }
            }
            Section {
                Label("Activity", systemImage: "clock.arrow.circlepath").tag("activity")
                Label("Component Gallery", systemImage: "paintpalette").tag("gallery")
            }
        }.listStyle(.sidebar).scrollContentBackground(.hidden).background(Palette.sidebar).tint(Palette.accent).navigationTitle("CleanYourMac")
    }
}

private struct OverviewView: View {
    @Bindable var store: AppStore
    @Namespace private var orb
    /// Until there is something to show, the orb is the page. Results appear as soon as the
    /// first ones arrive, while the scan carries on.
    private var hero: Bool { store.overview.findings == 0 && (store.isScanning || store.lastScanAt == nil) }
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xl) {
                if hero {
                    ScanHero(store: store, orb: orb)
                        .transition(.opacity)
                } else {
                    PageHeader("Your Mac, with room to work", subtitle: "Review the recommended fixes. Nothing is removed until you review it.")
                    ScanSummary(store: store, orb: orb)
                    DiskSpaceView(store: store)
                    WarningView(store: store)
                    RecommendationsView(store: store)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                    AllToolsView(store: store)
                }
            }
            .padding(Space.xl)
            .animation(.spring(duration: 0.7, bounce: 0.2), value: hero)
        }
        .task(id: store.storeVersion) {
            // Short enough to keep "found so far" moving while results stream in.
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled else { return }
            await store.refreshOverview()
        }
    }
}

/// Where to scan: the whole Mac or chosen folders.
private struct ScanPlacePicker: View {
    @Bindable var store: AppStore
    var body: some View {
        HStack(spacing: Space.md) {
            Picker("Scan", selection: $store.scanPlace) {
                ForEach(AppStore.ScanPlace.allCases, id: \.self) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented).labelsHidden().fixedSize()
            .disabled(store.isScanning, because: store.whileScanning)
            // Picking "Chosen folders" with none chosen asks for them straight away.
            .onChange(of: store.scanPlace) { _, place in
                if place == .folders && store.roots.isEmpty { store.chooseFolders() }
            }
            if store.scanPlace == .folders { ActionButton("Choose folders", disabled: store.isScanning, reason: store.whileScanning) { store.chooseFolders() } }
        }
    }
}
@MainActor private func placeDescription(_ store: AppStore) -> String {
    switch store.scanPlace {
    case .wholeMac: "Your home folder. System files and other drives are never scanned."
    case .folders: store.roots.isEmpty ? "No folders chosen yet" : store.roots.map { ($0 as NSString).abbreviatingWithTildeInPath }.joined(separator: " · ")
    }
}

/// The big orb, with the place to scan before and live progress during a scan.
private struct ScanHero: View {
    @Bindable var store: AppStore
    let orb: Namespace.ID
    var body: some View {
        VStack(spacing: Space.lg) {
            Text(store.isScanning ? "Looking around your Mac…" : "Your Mac, with room to work").font(TypeStyle.pageTitle)
            Text(store.isScanning ? "You can keep working; nothing is changed while scanning." : "One scan finds what developers and everyday use leave behind. You choose what to remove.")
                .font(TypeStyle.secondary).foregroundStyle(.secondary).multilineTextAlignment(.center)
            ScanOrb("Scan", subtitle: store.scanPlace.rawValue, phase: store.isScanning ? .scanning(progress: store.scanFraction) : .idle) { store.scan() }
                .matchedGeometryEffect(id: "orb", in: orb)
                .disabled(store.isApplying || store.demo, because: store.reason(store.inDemo, store.whileApplying))
            if store.isScanning {
                VStack(spacing: Space.sm) {
                    Text(store.progress).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                    HStack(spacing: Space.sm) {
                        StatChip("Found so far", value: Display.bytes(store.overview.diskBytes))
                        StatChip("Items", value: store.overview.findings.formatted())
                        if store.scanTotal > 1 { StatChip("Tools done", value: "\(store.scanFinished) of \(store.scanTotal)") }
                    }
                    ActionButton("Cancel") { store.cancelScan() }
                }
                .transition(.opacity)
            } else {
                ScanPlacePicker(store: store)
                Text(placeDescription(store)).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                DiskSpaceView(store: store).padding(.top, Space.lg)
            }
            WarningView(store: store)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, Space.xl)
    }
}

/// After a scan: the orb as a small "Scan again" button beside what was found.
private struct ScanSummary: View {
    @Bindable var store: AppStore
    let orb: Namespace.ID
    var body: some View {
        Panel {
            HStack(spacing: Space.lg) {
                ScanOrb("Scan again", phase: store.isScanning ? .scanning(progress: store.scanFraction) : .idle, diameter: Layout.scanOrbCompact) { store.scan() }
                    .matchedGeometryEffect(id: "orb", in: orb)
                    .disabled(store.isApplying || store.demo, because: store.reason(store.inDemo, store.whileApplying))
                VStack(alignment: .leading, spacing: Space.xs) {
                    Text("Found \(Display.bytes(store.overview.diskBytes)) in \(Display.items(store.overview.findings))\(store.isScanning ? " so far" : "")")
                        .font(TypeStyle.headline).contentTransition(.numericText())
                    if store.isScanning {
                        Text(store.scanTotal > 1 ? "\(store.scanFinished) of \(store.scanTotal) tools done · \(store.progress)" : store.progress)
                            .font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                    } else {
                        Text(lastScanned).font(TypeStyle.caption).foregroundStyle(.secondary)
                    }
                    Text(placeDescription(store)).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
                Spacer()
                if store.isScanning { ActionButton("Cancel") { store.cancelScan() } } else { ScanPlacePicker(store: store) }
            }
        }
    }
    private var lastScanned: String {
        guard let date = store.lastScanAt else { return "Not scanned yet" }
        return "Last scanned " + RelativeDateTimeFormatter().localizedString(for: date, relativeTo: Date())
    }
}

/// The home volume's capacity, with what the scan found and can free drawn into it.
private struct DiskSpaceView: View {
    @Bindable var store: AppStore
    var body: some View {
        if let disk = store.diskSpace {
            Panel {
                VStack(alignment: .leading, spacing: Space.md) {
                    HStack(alignment: .firstTextBaseline) {
                        Text("Your disk").font(TypeStyle.sectionTitle)
                        Spacer()
                        Text("\(Display.bytes(disk.used)) of \(Display.bytes(disk.total)) used").font(TypeStyle.caption).foregroundStyle(.secondary).monospacedDigit()
                    }
                    DiskBar(disk.breakdown(found: store.overview.diskBytes, ready: store.recommendedBytes)) { Display.bytes($0) }
                }
            }
        }
    }
}

/// The core's one-click fixes, largest first.
private struct RecommendationsView: View {
    @Bindable var store: AppStore
    var body: some View {
        if !store.recommendations.isEmpty {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(alignment: .firstTextBaseline, spacing: Space.md) {
                    VStack(alignment: .leading, spacing: Space.xs) {
                        Text("Recommended").font(TypeStyle.title)
                        Text(store.isScanning
                             ? "Free up \(Display.bytes(store.recommendedBytes)) so far · fixes open for review as each tool finishes"
                             : "Free up \(Display.bytes(store.recommendedBytes)) with these fixes")
                            .font(TypeStyle.secondary).foregroundStyle(.secondary).contentTransition(.numericText())
                    }
                    Spacer()
                    if store.recommendations.contains(where: { $0.action == .trash }) {
                        ActionButton("Review all", kind: .primary, disabled: store.isApplying || store.isScanning, reason: store.reason(store.whileApplying, store.whileScanning)) { store.reviewAllRecommendations() }
                    }
                }
                ForEach(store.recommendations) { fix in RecommendationCard(store: store, fix: fix) }
                    .animation(.spring(duration: 0.5), value: store.recommendations.map(\.id))
            }
        } else if store.lastScanAt != nil, !store.isScanning {
            Panel {
                Label("Nothing to recommend right now. Each tool below still lists everything it found.", systemImage: "checkmark.seal")
                    .font(TypeStyle.secondary).foregroundStyle(.secondary)
            }
        }
    }
}

private struct RecommendationCard: View {
    @Bindable var store: AppStore
    let fix: Recommendation
    var body: some View {
        Panel {
            HStack(alignment: .top, spacing: Space.lg) {
                RowIconView(icon: Display.icon(moduleID: fix.moduleID, path: nil, brand: fix.brand, symbol: store.modules.first { $0.id == fix.moduleID }?.symbol ?? "sparkles"))
                VStack(alignment: .leading, spacing: Space.xs) {
                    Text(fix.title).font(TypeStyle.sectionTitle)
                    Text(fix.detail).font(TypeStyle.secondary).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: Space.sm) {
                        StatusBadge(fix.risk.rawValue, warning: fix.risk == .permanent)
                        Text(Display.items(fix.count)).font(TypeStyle.caption).foregroundStyle(.secondary)
                        Button("Show items") { store.selectedModuleID = fix.moduleID }
                            .buttonStyle(.link).font(TypeStyle.caption)
                    }
                }
                Spacer()
                VStack(alignment: .trailing, spacing: Space.sm) {
                    Text(Display.bytes(fix.bytes)).font(TypeStyle.headline).monospacedDigit().contentTransition(.numericText())
                    if store.canReview(fix) {
                        ActionButton("Review", kind: fix.risk == .permanent ? .destructive : .secondary) { store.review(fix) }
                    } else {
                        HStack(spacing: Space.xs) {
                            ProgressView().controlSize(.small)
                            Text(store.isApplying ? "Action running" : "Still scanning").font(TypeStyle.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }
}

/// Every enabled tool, for looking closer than the recommendations do.
private struct AllToolsView: View {
    @Bindable var store: AppStore
    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            Text("All tools").font(TypeStyle.title)
            ForEach(Category.allCases, id: \.self) { category in
                let modules = store.enabledModules.filter { $0.category == category }
                if !modules.isEmpty {
                    Text(category.rawValue).font(TypeStyle.sectionTitle).foregroundStyle(.secondary)
                    ForEach(modules, id: \.id) { module in
                        Button { store.selectedModuleID = module.id; store.search = "" } label: {
                            Panel {
                                HStack(spacing: Space.lg) {
                                    Image(systemName: module.symbol).foregroundStyle(Palette.accentSymbol)
                                    VStack(alignment: .leading, spacing: Space.xs) {
                                        Text(module.name).font(TypeStyle.sectionTitle)
                                        Text(module.summary).font(TypeStyle.secondary).foregroundStyle(.secondary)
                                        if !module.inOverview {
                                            Text("Not part of the scan above. Open it to scan.").font(TypeStyle.caption).foregroundStyle(.secondary)
                                        }
                                    }
                                    Spacer()
                                    Text(store.count(for: module.id).formatted()).font(TypeStyle.numeric)
                                    Image(systemName: "chevron.right").foregroundStyle(.secondary)
                                }
                            }
                        }.buttonStyle(.plain)
                    }
                }
            }
        }
    }
}

private struct ScopeView: View {
    @Bindable var store: AppStore
    let usesRoots: Bool
    var body: some View {
        HStack(spacing: Space.md) {
            Image(systemName: usesRoots ? "folder" : "scope").foregroundStyle(.secondary)
            Text(usesRoots || store.scanPlace == .folders ? store.context.roots.map { ($0 as NSString).abbreviatingWithTildeInPath }.joined(separator: " · ") : scopeDescription)
                .font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(2).truncationMode(.middle)
            Spacer()
            if store.scanPlace == .folders {
                ActionButton("Whole Mac", disabled: store.isScanning || store.isApplying, reason: store.reason(store.whileScanning, store.whileApplying)) { store.scanPlace = .wholeMac }
            }
            if usesRoots || store.scanPlace == .folders { ActionButton("Choose folders", disabled: store.isScanning || store.isApplying, reason: store.reason(store.whileScanning, store.whileApplying)) {
                // On a tool's page, scan the new folders right away.
                if store.chooseFolders() { store.scan() }
            } }
        }
    }
    private var scopeDescription: String {
        switch store.selectedModuleID {
        case "orphans": "Current-user processes · refreshes while this view is open"
        case "simulators": "Devices and runtimes in your selected Xcode installation"
        case "xcode": "Your Xcode build and device-support folders"
        case "applications", "leftovers": "Installed applications and precisely named related files"
        case "downloads": "Your Downloads folder"
        case "trash": "Your local Trash"
        case "toolchains": "Version managers, SDK components and virtual devices in your home folder"
        default: "Known user caches and logs"
        }
    }
}

private struct FinderView: View {
    @Bindable var store: AppStore
    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: Space.lg) {
                PageHeader(store.currentModule?.name ?? "Findings", subtitle: store.currentModule?.summary ?? "")
                ScopeView(store: store, usesRoots: store.currentModule?.usesRoots ?? true)
                if store.selectedModuleID == "storage", !store.storageNavigation.isEmpty {
                    ActionButton("Back to parent", disabled: store.isScanning, reason: store.whileScanning) { store.browseBack() }
                }
                HStack(spacing: Space.md) {
                    TextField("Search results", text: $store.search).textFieldStyle(.roundedBorder)
                    Picker("Sort", selection: $store.sort) { ForEach(SortOrder.allCases, id: \.self) { Text($0.rawValue).tag($0) } }.fixedSize()
                    Button { store.sortAscending.toggle() } label: {
                        Image(systemName: store.sortAscending ? "arrow.up" : "arrow.down").foregroundStyle(Palette.accentSymbol)
                    }
                    .buttonStyle(.borderless)
                    .help(sortDirectionLabel)
                    .accessibilityLabel(sortDirectionLabel)
                    if store.isScanning { ActionButton("Cancel") { store.cancelScan() } }
                    else { ActionButton("Scan", kind: .primary, disabled: store.isApplying || store.demo, reason: store.reason(store.inDemo, store.whileApplying)) { store.scan() } }
                }
                SearchSummary(store: store)
                // Processes have no file dates or disk size; every other tool can be filtered.
                if store.selectedModuleID != "orphans" {
                    HStack(spacing: Space.md) {
                        Picker("Size", selection: $store.sizeFilter) { ForEach(SizeFilter.allCases, id: \.self) { Text($0.rawValue).tag($0) } }.fixedSize()
                        Picker("Modified", selection: $store.ageFilter) { ForEach(AgeFilter.allCases, id: \.self) { Text($0.rawValue).tag($0) } }.fixedSize()
                        Picker("Last used", selection: $store.usedFilter) { ForEach(AgeFilter.allCases, id: \.self) { Text($0.rawValue).tag($0) } }.fixedSize()
                        if store.sizeFilter != .all || store.ageFilter != .all || store.usedFilter != .all {
                            Button("Clear filters") { store.sizeFilter = .all; store.ageFilter = .all; store.usedFilter = .all }
                                .buttonStyle(.link)
                        }
                        Spacer()
                    }.font(TypeStyle.caption)
                }
                HStack {
                    if store.isScanning { ProgressView().controlSize(.small) }
                    Text(store.progress).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1)
                    Spacer()
                }
                WarningView(store: store)
                if store.selectedModuleID == "storage", !store.largest.isEmpty {
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Largest items · size on disk in this folder").font(TypeStyle.caption).foregroundStyle(.secondary)
                            ForEach(store.largest) { row in
                                StorageBar(row.title, value: Display.bytes(row.diskBytes), fraction: store.summary.diskBytes > 0 ? min(1, Double(row.diskBytes ?? 0) / Double(store.summary.diskBytes)) : 0)
                            }
                        }
                    }
                }
            }.padding(Space.xl)
            Divider()
            if store.resultTotal == 0 {
                EmptyState(store.isScanning ? "Looking for items…" : "No results to show", message: store.isScanning ? "Results appear as the scan progresses." : "Scan this tool or adjust your roots and search. Review warnings for incomplete coverage.", symbol: store.currentModule?.symbol ?? "tray")
            } else {
                SelectionBar(store: store)
                Divider()
                ResultsTable(store: store, symbol: store.currentModule?.symbol ?? "doc")
            }
            Divider()
            SelectionFooter(store: store)
        }
    }
    private var sortDirectionLabel: String {
        switch store.sort {
        case .name: store.sortAscending ? "Sorted A to Z; switch to Z to A" : "Sorted Z to A; switch to A to Z"
        case .size, .cpu: store.sortAscending ? "Smallest first; switch to largest first" : "Largest first; switch to smallest first"
        case .lastUsed: store.sortAscending ? "Least recently used first; switch to most recent first" : "Most recently used first; switch to least recent first"
        }
    }
}

/// Aggregate figures for the rows matching the current search and filters.
private struct SearchSummary: View {
    @Bindable var store: AppStore
    var body: some View {
        let summary = store.summary
        VStack(alignment: .leading, spacing: Space.xs) {
            if !store.search.isEmpty {
                Text("Matching “\(store.search)”").font(TypeStyle.caption).foregroundStyle(Palette.muted)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: Space.sm) {
                    StatChip(store.resultTotal == 1 ? "Item" : "Items", value: store.resultTotal.formatted())
                    if summary.processCount > 0 {
                        StatChip("Memory", value: Display.bytes(summary.processMemoryBytes))
                        if let busiest = store.maxCPU { StatChip("Highest CPU", value: String(format: "%.1f%%", busiest)) }
                    } else {
                        StatChip("Total size", value: Display.bytes(summary.diskBytes))
                        StatChip("Ready to review", value: Display.bytes(summary.reclaimableBytes), emphasized: summary.reclaimableBytes > 0)
                        if let largest = store.largest.first, let bytes = largest.diskBytes, bytes > 0 {
                            StatChip("Largest · " + largest.title, value: Display.bytes(bytes))
                        }
                    }
                    if summary.blocked > 0 { StatChip("Needs inspection", value: summary.blocked.formatted()) }
                    if !store.selectedIDs.isEmpty { StatChip("Selected", value: store.selectedIDs.count.formatted()) }
                }
            }
        }
        .task(id: store.queryKey) {
            // Debounce typing and streamed results before asking the core for a new snapshot.
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled else { return }
            await store.requery()
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Search summary")
    }
}

private struct WarningView: View {
    @Bindable var store: AppStore
    var body: some View {
        if !store.warnings.isEmpty {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: Space.sm) {
                    ForEach(Array(store.warnings.enumerated()), id: \.offset) { _, warning in Text(warning).font(TypeStyle.caption).textSelection(.enabled) }
                    Button("Open Full Disk Access Settings") {
                        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!)
                    }.font(TypeStyle.caption)
                }.padding(.top, Space.sm)
            } label: { Label("\(store.warnings.count) scan warnings · coverage may be incomplete", systemImage: "exclamationmark.triangle").font(TypeStyle.caption).foregroundStyle(Palette.warning) }
        }
    }
}

/// The select-all checkbox, the selection count and how to select with the keyboard.
private struct SelectionBar: View {
    @Bindable var store: AppStore
    var body: some View {
        HStack(spacing: Space.md) {
            Button { store.toggleSelectAll() } label: {
                HStack(spacing: Space.sm) {
                    Image(systemName: symbol)
                        .font(TypeStyle.headline)
                        .foregroundStyle(store.selectionState == .empty ? Palette.muted : Palette.accentSymbol)
                    Text(title).font(TypeStyle.label)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.borderless)
            .disabled(store.eligibleTotal == 0 || store.isApplying, because: store.reason(store.whileApplying, store.eligibleTotal == 0 ? "Nothing listed here can be acted on." : nil))
            .help(store.selectionState == .all ? "Clear the selection" : "Select every item that can be acted on")
            .accessibilityLabel(title)
            if !store.selectedIDs.isEmpty {
                Text("\(store.selectedIDs.count.formatted()) selected").font(TypeStyle.caption).foregroundStyle(Palette.muted)
                Button("Clear") { store.deselectAll() }.buttonStyle(.borderless).font(TypeStyle.caption).disabled(store.isApplying, because: store.whileApplying)
            }
            Spacer()
            Text("⌘-click to add · ⇧-click for a range · ⌘A for all · Esc to clear")
                .font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.head)
        }
        .padding(.horizontal, Space.lg)
        .padding(.vertical, Space.sm)
    }
    private var title: String {
        switch store.selectionState {
        case .all: "Deselect all"
        case .empty, .partial: store.eligibleTotal == store.resultTotal ? "Select all" : "Select all \(store.eligibleTotal.formatted()) eligible"
        }
    }
    private var symbol: String {
        switch store.selectionState {
        case .empty: "square"
        case .partial: "minus.square.fill"
        case .all: "checkmark.square.fill"
        }
    }
}

private struct SelectionFooter: View {
    @Bindable var store: AppStore
    var body: some View {
        HStack(spacing: Space.md) {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text("\(store.selectedIDs.count.formatted()) selected").font(TypeStyle.sectionTitle)
                Text(summary).font(TypeStyle.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if !store.selectedIDs.isEmpty { ActionButton("Clear", disabled: store.isApplying, reason: store.whileApplying) { store.selectedIDs.removeAll() } }
            ForEach(store.availableActions, id: \.self) { action in
                ActionButton("Review " + action.label, kind: action == .trash ? .primary : .destructive, disabled: store.isApplying || store.isScanning, reason: store.reason(store.whileApplying, store.whileScanning)) { store.reviewSelection(action) }
            }
        }
        .padding(Space.lg)
        .task(id: store.selectedIDs) {
            try? await Task.sleep(for: .milliseconds(120))
            guard !Task.isCancelled else { return }
            await store.refreshSelection()
        }
    }
    private var summary: String {
        if store.selection.includesProcesses { return "Process actions do not delete files or reclaim disk space." }
        if store.selectedIDs.isEmpty { return "Select items to review an action." }
        return Display.bytes(store.selection.bytes) + " on disk · review consequences before applying"
    }
}

private struct InspectorView: View {
    @Bindable var store: AppStore
    @State private var confirming: ActionKind?
    var body: some View {
        ScrollView {
            if let finding = store.inspected {
                VStack(alignment: .leading, spacing: Space.xl) {
                    PageHeader(finding.title, subtitle: finding.moduleID == "orphans" ? "Suspected orphan process" : "Item details")
                    HStack(spacing: Space.sm) {
                        RowIconView(icon: Display.icon(moduleID: finding.moduleID, path: finding.resource.path, brand: finding.brand, symbol: store.enabledModules.first { $0.id == finding.moduleID }?.symbol ?? "doc"))
                        StatusBadge(finding.risk.rawValue, warning: finding.risk == .permanent)
                        if let brand = finding.brand, let name = BrandCatalog.name(brand) {
                            Text(name).font(TypeStyle.caption).foregroundStyle(.secondary)
                        }
                    }
                    Text(finding.reason).font(TypeStyle.secondary)
                    if let blocked = finding.blockedReason { Label(blocked, systemImage: "lock").font(TypeStyle.secondary).foregroundStyle(Palette.warning) }
                    if finding.blockedReason == nil, let loss = finding.acknowledgement, let kind = finding.acknowledgedActions?.first {
                        Panel {
                            VStack(alignment: .leading, spacing: Space.md) {
                                Label("Protected", systemImage: "lock.shield").font(TypeStyle.sectionTitle).foregroundStyle(Palette.warning)
                                Text(loss).font(TypeStyle.secondary).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                                ActionButton("\(kind.label) Anyway…", kind: .destructive, disabled: store.isApplying || store.isScanning || store.demo, reason: store.reason(store.inDemo, store.whileApplying, store.whileScanning)) { confirming = kind }
                            }
                        }
                        .alert("\(kind.label) “\(finding.title)”?", isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } })) {
                            Button("I understand, \(kind.label.lowercased())", role: .destructive) {
                                if let kind = confirming { Task { await store.applyAcknowledged(finding, kind: kind) } }
                                confirming = nil
                            }
                            Button("Cancel", role: .cancel) { confirming = nil }
                        } message: {
                            Text(loss + (kind == .trash ? " You can restore it from Activity while it is still in the Trash." : ""))
                        }
                    }
                    if let allocated = finding.allocatedBytes { KeyValueRow("Size on disk", Display.bytes(allocated)) }
                    if let bytes = finding.bytes, bytes != finding.allocatedBytes {
                        // Sparse files (virtual disks, Docker) and APFS clones can be far larger logically.
                        KeyValueRow("Logical size", Display.bytes(bytes))
                    }
                    if let modified = finding.modifiedAt { KeyValueRow("Last modified", modified.formatted()) }
                    if let used = finding.lastUsedAt { KeyValueRow("Last used", used.formatted() + " · " + used.formatted(.relative(presentation: .named))) }
                    if let memory = finding.memoryBytes { KeyValueRow("Memory footprint", Display.bytes(memory)) }
                    if let path = finding.resource.path { KeyValueRow("Path", path) }
                    ForEach(Array(finding.details.enumerated()), id: \.offset) { _, detail in KeyValueRow(detail.label, detail.value) }
                    ActionButton("Reveal in Finder") { store.reveal(id: finding.id, path: finding.resource.path) }
                    if finding.moduleID == "storage", let path = finding.resource.path, store.core.isDirectory(path) {
                        ActionButton("Inspect folder", kind: .primary, disabled: store.isScanning || store.demo, reason: store.reason(store.inDemo, store.whileScanning)) { store.browse(path) }
                    }
                    if case .process = finding.resource { ActionButton("Ignore process name") { store.ignore(processName: finding.title, id: finding.id) } }
                    else { ActionButton("Protect this path") { store.protect(path: finding.resource.path, id: finding.id) } }
                }.padding(Space.xl)
            }
        }.background(Palette.surface)
    }
}

private struct ReviewView: View {
    @Bindable var store: AppStore
    let draft: ReviewDraft
    var body: some View {
        VStack(alignment: .leading, spacing: Space.xl) {
            PageHeader(draft.kind.label, subtitle: Display.items(draft.selection.count) + " selected for review · " + Display.bytes(draft.selection.bytes))
            Panel { Label(draft.kind.consequence, systemImage: "exclamationmark.circle").font(TypeStyle.secondary).fixedSize(horizontal: false, vertical: true) }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: Space.lg) {
                    ForEach(draft.selection.preview) { finding in
                        VStack(alignment: .leading, spacing: Space.xs) {
                            Text(finding.title).font(TypeStyle.sectionTitle)
                            Text(finding.resource.path ?? finding.subtitle).font(TypeStyle.code).textSelection(.enabled)
                            Text(finding.reason).font(TypeStyle.caption).foregroundStyle(.secondary)
                        }
                        Divider()
                    }
                    if draft.selection.count > draft.selection.preview.count {
                        Text("and \((draft.selection.count - draft.selection.preview.count).formatted()) more").font(TypeStyle.caption).foregroundStyle(Palette.muted)
                    }
                }
            }
            HStack {
                if store.isApplying { ProgressView().controlSize(.small); Text("Applying reviewed actions…").font(TypeStyle.caption) }
                Spacer()
                ActionButton("Cancel", disabled: store.isApplying, reason: store.whileApplying) { store.review = nil }
                ActionButton(draft.kind.label, kind: .destructive, disabled: store.isApplying || store.demo, reason: store.reason(store.inDemo, store.whileApplying)) { Task { await store.apply(draft) } }
            }
        }.padding(Space.xl).frame(width: Layout.reviewWidth, height: Layout.reviewHeight).interactiveDismissDisabled(store.isApplying)
    }
}

private struct ActivityView: View {
    @Bindable var store: AppStore
    var body: some View {
        VStack(alignment: .leading, spacing: Space.xl) {
            HStack {
                PageHeader("Activity", subtitle: "Local action outcomes. Process command arguments are not stored.")
                if !store.history.isEmpty { ActionButton("Clear history", disabled: store.isApplying || store.demo, reason: store.reason(store.inDemo, store.whileApplying)) { Task { await store.clearHistory() } } }
            }
            if store.history.isEmpty { EmptyState("No actions yet", message: "Reviewed actions and their results appear here.", symbol: "clock") }
            else {
                ScrollView {
                    LazyVStack(spacing: Space.md) {
                        ForEach(store.history) { row in
                            Panel {
                                VStack(alignment: .leading, spacing: Space.sm) {
                                    HStack {
                                        Text(row.title).font(TypeStyle.sectionTitle)
                                        Spacer()
                                        StatusBadge(row.outcome.rawValue.capitalized, warning: row.outcome == .failed)
                                    }
                                    Text(row.action.label + " · " + row.date.formatted()).font(TypeStyle.caption).foregroundStyle(.secondary)
                                    Text(row.message).font(TypeStyle.secondary)
                                    if let warning = row.journalWarning { Text(warning).font(TypeStyle.caption).foregroundStyle(Palette.warning) }
                                    if let path = row.originalPath { Text(path).font(TypeStyle.code).textSelection(.enabled) }
                                    if let trash = row.trashPath, row.trashIdentity != nil, FileManager.default.fileExists(atPath: trash), !store.restoredIDs.contains(row.id) {
                                        ActionButton("Restore", disabled: store.demo || store.isApplying, reason: store.reason(store.inDemo, store.whileApplying)) { Task { await store.restore(row) } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }.padding(Space.xl).task { if !store.demo { await store.loadHistory() } }
    }
}

public struct PreferencesView: View {
    @Bindable private var store: AppStore
    @State private var ignoredName = ""
    public init(store: AppStore) { self.store = store }
    public var body: some View {
        TabView {
            general.tabItem { Label("General", systemImage: "gearshape") }
            protection.tabItem { Label("Protection", systemImage: "lock.shield") }
            modules.tabItem { Label("Modules", systemImage: "square.stack.3d.up") }
            appearance.tabItem { Label("Appearance", systemImage: "paintpalette") }
        }
        .frame(width: Layout.reviewWidth, height: Layout.reviewHeight)
    }

    private var general: some View {
        Form {
            Section {
                if store.roots.isEmpty { Text("No folders yet.").foregroundStyle(.secondary) }
                ForEach(store.roots, id: \.self) { root in
                    LabeledContent {
                        Button("Remove") { store.roots.removeAll { $0 == root }; store.storageNavigation = []; store.persist() }
                    } label: { PathText(root) }
                }
                Button("Add Folders…") { store.addRoot() }
            } header: {
                Text("Scan roots")
            } footer: {
                Text("Scans look only inside these folders when you choose Chosen folders. Adding one switches to it.").font(TypeStyle.caption).foregroundStyle(.secondary)
            }
            Section("Menu bar") {
                Toggle("Show a menu bar summary and monitor orphan processes", isOn: $store.menuBarEnabled)
            }
        }
        .formStyle(.grouped)
    }

    private var protection: some View {
        Form {
            Section {
                if store.exclusions.isEmpty {
                    Text("Protect a finding from its inspector or context menu.").foregroundStyle(.secondary)
                }
                ForEach(store.exclusions, id: \.self) { path in
                    LabeledContent {
                        Button("Unprotect") { store.exclusions.removeAll { $0 == path }; store.persist() }
                    } label: { PathText(path) }
                }
            } header: {
                Text("Protected paths")
            } footer: {
                Text("Protected items and everything inside them are never offered for removal.").font(TypeStyle.caption).foregroundStyle(.secondary)
            }
            Section("Ignored process names") {
                ForEach(store.ignoredNames, id: \.self) { name in
                    LabeledContent {
                        Button("Show Again") { store.ignoredNames.removeAll { $0 == name }; store.persist() }
                    } label: { Text(name).font(TypeStyle.code).lineLimit(1).truncationMode(.middle) }
                }
                HStack {
                    TextField("Executable name", text: $ignoredName, prompt: Text("Executable name"))
                        .labelsHidden()
                        .onSubmit(ignore)
                    Button("Ignore", action: ignore).disabled(ignoredName.trimmingCharacters(in: .whitespaces).isEmpty, because: "Type an executable name first.")
                }
            }
        }
        .formStyle(.grouped)
    }

    private var modules: some View {
        Form {
            ForEach(Category.allCases, id: \.self) { category in
                Section(category.rawValue) {
                    ForEach(store.modules.filter { $0.category == category }, id: \.id) { module in
                        Toggle(isOn: Binding(get: { !store.disabledModules.contains(module.id) }, set: { enabled in
                            store.disabledModules.removeAll { $0 == module.id }
                            if !enabled { store.disabledModules.append(module.id) }
                            if !enabled && store.selectedModuleID == module.id { store.selectedModuleID = "overview" }
                            store.persist()
                        })) {
                            Label {
                                VStack(alignment: .leading, spacing: Space.xxs) {
                                    Text(module.name)
                                    Text(module.summary).font(TypeStyle.caption).foregroundStyle(.secondary)
                                }
                            } icon: {
                                Image(systemName: module.symbol).foregroundStyle(Palette.accentSymbol)
                            }
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
    }

    private var appearance: some View {
        Form {
            Section("Palette") {
                Picker("Palette", selection: $store.theme) {
                    ForEach(ComfyTheme.allCases) { theme in
                        HStack(spacing: Space.sm) {
                            ThemeSwatch(theme)
                            Text(theme.rawValue)
                        }
                        .tag(theme)
                    }
                }
                .pickerStyle(.radioGroup)
                .labelsHidden()
            }
        }
        .formStyle(.grouped)
    }

    private func ignore() {
        let name = ignoredName.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, !store.ignoredNames.contains(name) else { return }
        store.ignoredNames.append(name); store.persist(); ignoredName = ""
    }
}

/// A path shown with `~` for the home folder, truncated in the middle rather than widening its row.
private struct PathText: View {
    let path: String
    init(_ path: String) { self.path = path }
    var body: some View {
        Text((path as NSString).abbreviatingWithTildeInPath)
            .font(TypeStyle.code).lineLimit(1).truncationMode(.middle).help(path)
    }
}
