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
            // The page floats as a rounded sheet on the window, the app's signature shape. All
            // SwiftUI, so it keeps inside the safe area that newer macOS uses to slide content
            // under the sidebar and toolbar (an AppKit split view here ignored it).
            content
                .frame(minWidth: Layout.contentMin, maxWidth: .infinity, maxHeight: .infinity)
                .background(Palette.canvas)
            .clipShape(RoundedRectangle(cornerRadius: Layout.sheetRadius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Layout.sheetRadius, style: .continuous).strokeBorder(Palette.border, lineWidth: Stroke.hairline))
            .softShadow(0.8)
            .padding([.trailing, .bottom], Space.sm)
            .background(Palette.sidebar)
            .inspector(isPresented: inspectorShown) {
                InspectorView(store: store)
                    .inspectorColumnWidth(min: Layout.inspectorMin, ideal: Layout.inspector, max: Layout.inspectorMax)
            }
        }
        .frame(minWidth: Layout.windowMinWidth, minHeight: Layout.windowMinHeight)
        .background(Palette.sidebar)
        .id(store.theme)
        .toolbar {
            ToolbarItemGroup {
                if store.demo { Text("Demo · actions disabled").font(TypeStyle.caption).foregroundStyle(Palette.muted) }
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
    /// The inspector shows the inspected item on tool pages; closing it hides it until toggled.
    private var inspectorShown: Binding<Bool> {
        Binding(
            get: { store.showInspector && store.inspected != nil && store.selectedModuleID != "activity" && store.selectedModuleID != "gallery" },
            set: { if !$0 { store.showInspector = false } }
        )
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
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xxs) {
                BrandMark().padding(.horizontal, Space.sm).padding(.top, Space.xs).padding(.bottom, Space.md)
                item("Overview", symbol: "square.grid.2x2", id: "overview", tone: .gray)
                ForEach(Category.allCases, id: \.self) { category in
                    let modules = store.enabledModules.filter { $0.category == category }
                    if !modules.isEmpty {
                        SidebarHeading(category.rawValue)
                        ForEach(modules, id: \.id) { module in item(module.name, symbol: module.symbol, id: module.id) }
                    }
                }
                SidebarHeading("History")
                item("Activity", symbol: "clock.arrow.circlepath", id: "activity")
                item("Component Gallery", symbol: "paintpalette", id: "gallery")
            }
            .padding(.horizontal, Space.sm).padding(.bottom, Space.lg)
        }
        .background(Palette.sidebar)
        .navigationTitle("CleanYourMac")
    }
    private func item(_ title: String, symbol: String, id: String, tone: IconTone? = nil) -> some View {
        SidebarItem(title, symbol: symbol, selected: (store.selectedModuleID ?? "overview") == id, tone: tone ?? Display.tone(id)) { store.selectedModuleID = id }
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
            PillSegments(selection: $store.scanPlace, options: AppStore.ScanPlace.allCases.map { ChoiceOption($0, $0.rawValue) })
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
                .font(TypeStyle.secondary).foregroundStyle(Palette.muted).multilineTextAlignment(.center)
            ScanOrb("Scan", subtitle: store.scanPlace.rawValue, phase: store.isScanning ? .scanning(progress: store.scanFraction) : .idle) { store.scan() }
                .matchedGeometryEffect(id: "orb", in: orb)
                .disabled(store.isApplying || store.demo, because: store.reason(store.inDemo, store.whileApplying))
            if store.isScanning {
                VStack(spacing: Space.sm) {
                    Text(store.progress).font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.middle)
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
                Text(placeDescription(store)).font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.middle)
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
                            .font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.middle)
                    } else {
                        Text(lastScanned).font(TypeStyle.caption).foregroundStyle(Palette.muted)
                    }
                    Text(placeDescription(store)).font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.middle)
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
                        Text("\(Display.bytes(disk.used)) of \(Display.bytes(disk.total)) used").font(TypeStyle.caption).foregroundStyle(Palette.muted).monospacedDigit()
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
                            .font(TypeStyle.secondary).foregroundStyle(Palette.muted).contentTransition(.numericText())
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
                    .font(TypeStyle.secondary).foregroundStyle(Palette.muted)
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
                IconTile(icon: Display.icon(moduleID: fix.moduleID, path: nil, brand: fix.brand, symbol: store.modules.first { $0.id == fix.moduleID }?.symbol ?? "sparkles"), tone: Display.tone(fix.moduleID))
                VStack(alignment: .leading, spacing: Space.xs) {
                    Text(fix.title).font(TypeStyle.sectionTitle)
                    Text(fix.detail).font(TypeStyle.secondary).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: Space.sm) {
                        StatusBadge(fix.risk.rawValue, tone: Display.riskTone(fix.risk) ?? Palette.muted, warning: fix.risk == .permanent)
                        Text(Display.items(fix.count)).font(TypeStyle.caption).foregroundStyle(Palette.muted)
                        TextAction("Show items") { store.selectedModuleID = fix.moduleID }
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
                            Text(store.isApplying ? "Action running" : "Still scanning").font(TypeStyle.caption).foregroundStyle(Palette.muted)
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
                    Text(category.rawValue).font(TypeStyle.sectionTitle).foregroundStyle(Palette.muted)
                    ForEach(modules, id: \.id) { module in
                        Button { store.selectedModuleID = module.id; store.search = "" } label: {
                            Panel {
                                HStack(spacing: Space.lg) {
                                    ToolIcon(module.symbol, tone: Display.tone(module.id))
                                    VStack(alignment: .leading, spacing: Space.xs) {
                                        Text(module.name).font(TypeStyle.sectionTitle)
                                        Text(module.summary).font(TypeStyle.secondary).foregroundStyle(Palette.muted)
                                        if !module.inOverview {
                                            Text("Not part of the scan above. Open it to scan.").font(TypeStyle.caption).foregroundStyle(Palette.muted)
                                        }
                                    }
                                    Spacer()
                                    Text(store.count(for: module.id).formatted()).font(TypeStyle.numeric)
                                    Image(systemName: "chevron.right").foregroundStyle(Palette.muted)
                                }
                            }
                        }.buttonStyle(.plain)
                    }
                }
            }
        }
    }
}

/// Where scans look, on every page: the whole Mac or the chosen folders, which can be
/// changed or re-chosen here.
private struct ScopeChip: View {
    @Bindable var store: AppStore
    @State private var open = false
    private var value: String {
        guard store.scanPlace == .folders else { return "Whole Mac" }
        let names = store.roots.map { ($0 as NSString).lastPathComponent }
        guard let first = names.first else { return "No folders" }
        return names.count > 1 ? "\(first) +\(names.count - 1)" : first
    }
    var body: some View {
        Button { open.toggle() } label: {
            ChipLabel(symbol: store.scanPlace == .folders ? "folder" : "laptopcomputer", label: "Scanning", value: value, active: store.scanPlace == .folders, open: open)
        }
        .buttonStyle(.plain)
        .disabled(store.isScanning || store.isApplying, because: store.reason(store.whileScanning, store.whileApplying))
        .help(placeDescription(store))
        .popover(isPresented: $open, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                OptionRow(title: "Whole Mac", checked: store.scanPlace == .wholeMac) { store.scanPlace = .wholeMac; open = false }
                OptionRow(title: store.roots.isEmpty ? "Chosen folders" : "Chosen folders (\(store.roots.count))", checked: store.scanPlace == .folders) {
                    open = false
                    if store.roots.isEmpty { store.chooseFolders() } else { store.scanPlace = .folders }
                }
                ForEach(store.roots, id: \.self) { root in
                    Text((root as NSString).abbreviatingWithTildeInPath).font(TypeStyle.caption).foregroundStyle(Palette.muted)
                        .lineLimit(1).truncationMode(.middle).padding(.leading, Space.xl + Space.xs)
                }
                Divider().padding(.vertical, Space.xxs)
                OptionRow(title: "Choose folders…", symbol: "plus") {
                    open = false
                    // On a tool's page, scan the new folders right away.
                    if store.chooseFolders(), store.selectedModuleID != nil, store.selectedModuleID != "overview" { store.scan() }
                }
            }
            .padding(Space.xs).frame(minWidth: Layout.menuMinWidth, maxWidth: Layout.menuMaxWidth).background(Palette.elevated)
        }
    }
}

private struct FinderView: View {
    @Bindable var store: AppStore
    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: Space.md) {
                ToolHeader(store.currentModule?.name ?? "Findings", subtitle: store.currentModule?.summary ?? "", symbol: store.currentModule?.symbol ?? "tray", tone: store.currentModule.map { Display.tone($0.id) }) {
                    if store.isScanning {
                        HStack(spacing: Space.sm) {
                            ProgressView().controlSize(.small)
                            ActionButton("Cancel") { store.cancelScan() }
                        }
                    } else {
                        ActionButton("Scan", kind: .primary, disabled: store.isApplying || store.demo, reason: store.reason(store.inDemo, store.whileApplying)) { store.scan() }
                    }
                }
                HStack(spacing: Space.sm) {
                    ScopeChip(store: store)
                    if store.selectedModuleID == "storage", !store.storageNavigation.isEmpty {
                        Button { store.browseBack() } label: { ChipLabel(symbol: "chevron.left", value: "Parent folder") }
                            .buttonStyle(.plain).disabled(store.isScanning, because: store.whileScanning)
                    }
                    Spacer(minLength: Space.md)
                    SearchSummary(store: store)
                }
                HStack(spacing: Space.sm) {
                    SearchField("Search by name or path", text: $store.search).frame(minWidth: Layout.searchMinWidth, maxWidth: Layout.searchMaxWidth)
                    // Processes have no file dates or disk size; every other tool can be filtered.
                    if store.selectedModuleID == "ai" {
                        ChoiceChip("Tier", selection: $store.tierFilter, neutral: .all, options: TierFilter.allCases.map { ChoiceOption($0, $0.rawValue) })
                    }
                    if store.selectedModuleID != "orphans" {
                        ChoiceChip("Size", selection: $store.sizeFilter, neutral: .all, options: SizeFilter.allCases.map { ChoiceOption($0, $0.rawValue) })
                        ChoiceChip("Modified", selection: $store.ageFilter, neutral: .all, options: AgeFilter.allCases.map { ChoiceOption($0, $0.rawValue) })
                        ChoiceChip("Last used", selection: $store.usedFilter, neutral: .all, options: AgeFilter.allCases.map { ChoiceOption($0, $0.rawValue) })
                        if store.sizeFilter != .all || store.ageFilter != .all || store.usedFilter != .all || store.tierFilter != .all {
                            Button("Clear") { store.sizeFilter = .all; store.ageFilter = .all; store.usedFilter = .all; store.tierFilter = .all }
                                .buttonStyle(.plain).font(TypeStyle.label).foregroundStyle(Palette.accentText)
                        }
                    }
                    Spacer(minLength: Space.sm)
                    ChoiceChip("Sort", symbol: nil, selection: $store.sort, options: SortOrder.allCases.map { ChoiceOption($0, $0.rawValue) })
                    Button { store.sortAscending.toggle() } label: {
                        Image(systemName: store.sortAscending ? "arrow.up" : "arrow.down").font(TypeStyle.label)
                            .foregroundStyle(Palette.accentSymbol).frame(width: Layout.chipHeight, height: Layout.chipHeight)
                            .background(Palette.surface, in: Circle()).overlay(Circle().strokeBorder(Palette.border, lineWidth: Stroke.hairline))
                    }
                    .buttonStyle(.plain).help(sortDirectionLabel).accessibilityLabel(sortDirectionLabel)
                }
                if store.isScanning {
                    Text(store.progress).font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1)
                }
                WarningView(store: store)
                if store.selectedModuleID == "storage", !store.largest.isEmpty {
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Largest items · size on disk in this folder").font(TypeStyle.caption).foregroundStyle(Palette.muted)
                            ForEach(store.largest) { row in
                                StorageBar(row.title, value: Display.bytes(row.diskBytes), fraction: store.summary.diskBytes > 0 ? min(1, Double(row.diskBytes ?? 0) / Double(store.summary.diskBytes)) : 0)
                            }
                        }
                    }
                }
            }.padding([.horizontal, .top], Space.xl).padding(.bottom, Space.md)
            VStack(spacing: 0) {
                if store.resultTotal == 0 {
                    EmptyState(store.isScanning ? "Looking for items…" : "Nothing here yet", message: store.isScanning ? "Results appear as the scan progresses." : "Scan this tool, or widen the folders and search. Warnings explain anything that couldn't be read.", symbol: store.currentModule?.symbol ?? "tray")
                } else {
                    SelectionBar(store: store)
                    Divider().padding(.horizontal, Space.lg)
                    ResultsTable(store: store, symbol: store.currentModule?.symbol ?? "doc").padding(.vertical, Space.xs)
                }
            }
            .card(radius: Layout.panelRadius)
            .padding(.horizontal, Space.xl)
            SelectionFooter(store: store).padding(.horizontal, Space.lg).padding(.vertical, Space.sm)
        }
        // The page owns the query: whenever the tool, search, filters, sort or stored results
        // change, ask the core for a new snapshot (debounced for typing and streamed results).
        .task(id: store.queryKey) {
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled else { return }
            await store.requery()
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

/// Aggregate figures for the rows matching the current search and filters, in one line.
private struct SearchSummary: View {
    @Bindable var store: AppStore
    var body: some View {
        let summary = store.summary
        var parts = [SummaryLine.Part(store.resultTotal.formatted(), store.resultTotal == 1 ? "item" : "items")]
        if summary.processCount > 0 {
            parts.append(.init(Display.bytes(summary.processMemoryBytes), "memory"))
            if let busiest = store.maxCPU { parts.append(.init(String(format: "%.1f%%", busiest), "highest CPU")) }
        } else if store.resultTotal > 0 {
            parts.append(.init(Display.bytes(summary.diskBytes), "on disk"))
            parts.append(.init(Display.bytes(summary.reclaimableBytes), "to review", emphasized: summary.reclaimableBytes > 0))
        }
        if summary.blocked > 0 { parts.append(.init(summary.blocked.formatted(), "need inspection")) }
        if !store.selectedIDs.isEmpty { parts.append(.init(store.selectedIDs.count.formatted(), "selected", emphasized: true)) }
        if !store.search.isEmpty { parts.append(.init("“\(store.search)”", "matching")) }
        return SummaryLine(parts)
    }
}

private struct WarningView: View {
    @Bindable var store: AppStore
    /// Whether some folders could not be read because the app lacks Full Disk Access.
    private var needsAccess: Bool {
        store.warnings.contains { $0.contains("Full Disk Access") || $0.contains("not permitted") || $0.contains("Permission denied") }
    }
    var body: some View {
        if !store.warnings.isEmpty {
            Callout(
                store.warnings.count == 1 ? "1 scan warning · coverage may be incomplete" : "\(store.warnings.count) scan warnings · coverage may be incomplete",
                symbol: "exclamationmark.triangle.fill", tone: Palette.warning,
                action: needsAccess ? (title: "Grant Full Disk Access", run: {
                    NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!)
                }) : nil
            ) {
                LazyVStack(alignment: .leading, spacing: Space.sm) {
                    ForEach(Array(store.warnings.enumerated()), id: \.offset) { _, warning in Text(warning).font(TypeStyle.caption).foregroundStyle(Palette.muted).textSelection(.enabled) }
                }
            }
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
            .buttonStyle(.plain)
            .disabled(store.eligibleTotal == 0 || store.isApplying, because: store.reason(store.whileApplying, store.eligibleTotal == 0 ? "Nothing listed here can be acted on." : nil))
            .help(store.selectionState == .all ? "Clear the selection" : "Select every item that can be acted on")
            .accessibilityLabel(title)
            if !store.selectedIDs.isEmpty {
                Text("\(store.selectedIDs.count.formatted()) selected").font(TypeStyle.caption).foregroundStyle(Palette.muted)
                TextAction("Clear") { store.deselectAll() }.disabled(store.isApplying, because: store.whileApplying)
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
                Text(summary).font(TypeStyle.caption).foregroundStyle(Palette.muted)
            }
            Spacer()
            if !store.selectedIDs.isEmpty { ActionButton("Clear", disabled: store.isApplying, reason: store.whileApplying) { store.selectedIDs.removeAll() } }
            ForEach(store.availableActions, id: \.self) { action in
                ActionButton("Review " + action.label, kind: action == .trash ? .primary : .destructive, disabled: store.isApplying || store.isScanning, reason: store.reason(store.whileApplying, store.whileScanning)) { store.reviewSelection(action) }
            }
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.md)
        // A floating action card that lifts once something is selected.
        .card(radius: Layout.panelRadius, elevation: store.selectedIDs.isEmpty ? 0.3 : 1.2)
        .animation(Motion.press, value: store.selectedIDs.isEmpty)
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
                        IconTile(icon: Display.icon(moduleID: finding.moduleID, path: finding.resource.path, brand: finding.brand, symbol: store.enabledModules.first { $0.id == finding.moduleID }?.symbol ?? "doc"), tone: Display.tone(finding.moduleID))
                        let confirm = finding.blockedReason == nil && finding.acknowledgement != nil
                        let shown = Display.badge(risk: finding.risk, badge: finding.badge, blocked: finding.blockedReason != nil, confirm: confirm)
                        StatusBadge(shown.text, tone: shown.tone ?? Palette.muted, warning: finding.risk == .permanent || confirm)
                        if let brand = finding.brand, let name = BrandCatalog.name(brand) {
                            Text(name).font(TypeStyle.caption).foregroundStyle(Palette.muted)
                        }
                    }
                    Text(finding.reason).font(TypeStyle.secondary)
                    if let blocked = finding.blockedReason { Label(blocked, systemImage: "lock").font(TypeStyle.secondary).foregroundStyle(Palette.warning) }
                    if finding.blockedReason == nil, let loss = finding.acknowledgement, let kind = finding.acknowledgedActions?.first {
                        Panel {
                            VStack(alignment: .leading, spacing: Space.md) {
                                Label("Confirm what is lost", systemImage: "lock.shield").font(TypeStyle.sectionTitle).foregroundStyle(Palette.warning)
                                Text(loss).font(TypeStyle.secondary).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
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
                    Panel {
                    VStack(alignment: .leading, spacing: Space.md) {
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
                    }
                    }
                    ActionButton("Reveal in Finder") { store.reveal(id: finding.id, path: finding.resource.path) }
                    if finding.moduleID == "storage", let path = finding.resource.path, store.core.isDirectory(path) {
                        ActionButton("Inspect folder", kind: .primary, disabled: store.isScanning || store.demo, reason: store.reason(store.inDemo, store.whileScanning)) { store.browse(path) }
                    }
                    if case .process = finding.resource { ActionButton("Ignore process name") { store.ignore(processName: finding.title, id: finding.id) } }
                    else { ActionButton("Protect this path") { store.protect(path: finding.resource.path, id: finding.id) } }
                }.padding(Space.xl)
            }
        }.background(Palette.canvas)
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
                            Text(finding.reason).font(TypeStyle.caption).foregroundStyle(Palette.muted)
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
                                    Text(row.action.label + " · " + row.date.formatted()).font(TypeStyle.caption).foregroundStyle(Palette.muted)
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
                if store.roots.isEmpty { Text("No folders yet.").foregroundStyle(Palette.muted) }
                ForEach(store.roots, id: \.self) { root in
                    LabeledContent {
                        Button("Remove") { store.roots.removeAll { $0 == root }; store.storageNavigation = []; store.persist() }
                    } label: { PathText(root) }
                }
                Button("Add Folders…") { store.addRoot() }
            } header: {
                Text("Scan roots")
            } footer: {
                Text("Scans look only inside these folders when you choose Chosen folders. Adding one switches to it.").font(TypeStyle.caption).foregroundStyle(Palette.muted)
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
                    Text("Protect a finding from its inspector or context menu.").foregroundStyle(Palette.muted)
                }
                ForEach(store.exclusions, id: \.self) { path in
                    LabeledContent {
                        Button("Unprotect") { store.exclusions.removeAll { $0 == path }; store.persist() }
                    } label: { PathText(path) }
                }
            } header: {
                Text("Protected paths")
            } footer: {
                Text("Protected items and everything inside them are never offered for removal.").font(TypeStyle.caption).foregroundStyle(Palette.muted)
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
                                    Text(module.summary).font(TypeStyle.caption).foregroundStyle(Palette.muted)
                                }
                            } icon: {
                                ToolIcon(module.symbol, tone: Display.tone(module.id), size: .small)
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
            Section("Appearance") {
                PillSegments(selection: $store.appearance, options: AppStore.Appearance.allCases.map { ChoiceOption($0, $0.rawValue) })
                Text("Every palette has a light and a dark version. System follows your Mac's setting.")
                    .font(TypeStyle.caption).foregroundStyle(Palette.muted)
            }
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
