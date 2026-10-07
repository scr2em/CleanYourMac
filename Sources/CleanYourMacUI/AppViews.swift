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
                Button { store.showSettings = true } label: { Label("Settings", systemImage: "gearshape") }
            }
        }
        .sheet(isPresented: $store.showSettings) { PreferencesView(store: store) }
        .sheet(item: $store.review) { draft in ReviewView(store: store, draft: draft) }
        .alert("Attention needed", isPresented: Binding(get: { store.error != nil }, set: { if !$0 { store.error = nil } })) {
            Button("OK") { store.error = nil }
        } message: { Text(store.error ?? "") }
        .task { if !store.demo { await store.loadHistory() } }
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
        }.listStyle(.sidebar).scrollContentBackground(.hidden).background(Palette.sidebar).navigationTitle("CleanYourMac")
    }
}

private struct OverviewView: View {
    @Bindable var store: AppStore
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xl) {
                PageHeader("Your Mac, with room to work", subtitle: "Inspect storage and developer clutter. Every removal starts with your selection.")
                HStack {
                    MetricTile("Scan results", value: String(store.findings.count), detail: "Across enabled modules")
                    MetricTile("Ready to review", value: Display.bytes(store.analytics.reclaimableBytes), detail: "Eligible items, overlaps counted once")
                    MetricTile("Found on disk", value: Display.bytes(store.analytics.diskBytes), detail: "Logical size of scanned items")
                    MetricTile("Root folders", value: String(store.roots.count), detail: "Chosen by you")
                }
                ScopeView(store: store, usesRoots: true)
                HStack {
                    if store.isScanning { ProgressView().controlSize(.small); Text(store.progress).font(TypeStyle.caption); Spacer(); ActionButton("Cancel") { store.cancelScan() } }
                    else { ActionButton("Scan enabled modules", kind: .primary, disabled: store.isApplying || store.demo) { store.scan() }; Text(store.progress).font(TypeStyle.caption).foregroundStyle(.secondary) }
                }
                WarningView(store: store)
                ForEach(Category.allCases, id: \.self) { category in
                    VStack(alignment: .leading, spacing: Space.md) {
                        Text(category.rawValue).font(TypeStyle.sectionTitle)
                        ForEach(store.enabledModules.filter { $0.category == category }, id: \.id) { module in
                            Button { store.selectedModuleID = module.id; store.search = "" } label: {
                                Panel {
                                    HStack(spacing: Space.lg) {
                                        Image(systemName: module.symbol).foregroundStyle(Palette.accentSymbol)
                                        VStack(alignment: .leading, spacing: Space.xs) {
                                            Text(module.name).font(TypeStyle.sectionTitle)
                                            Text(module.summary).font(TypeStyle.secondary).foregroundStyle(.secondary)
                                        }
                                        Spacer()
                                        Text(String(store.findings.filter { $0.moduleID == module.id }.count)).font(TypeStyle.body).monospacedDigit()
                                        Image(systemName: "chevron.right").foregroundStyle(.secondary)
                                    }
                                }
                            }.buttonStyle(.plain)
                        }
                    }
                }
            }.padding(Space.xl)
        }
    }
}

private struct ScopeView: View {
    @Bindable var store: AppStore
    let usesRoots: Bool
    var body: some View {
        HStack(spacing: Space.md) {
            Image(systemName: usesRoots ? "folder" : "scope").foregroundStyle(.secondary)
            Text(usesRoots ? store.context.roots.map { ($0 as NSString).abbreviatingWithTildeInPath }.joined(separator: " · ") : scopeDescription)
                .font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(2).truncationMode(.middle)
            Spacer()
            if usesRoots { ActionButton("Choose folders", disabled: store.isScanning || store.isApplying) { store.addRoot() } }
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
                    ActionButton("Back to parent", disabled: store.isScanning) { store.browseBack() }
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
                    else { ActionButton("Scan", kind: .primary, disabled: store.isApplying || store.demo) { store.scan() } }
                }
                SearchSummary(store: store)
                if ["large", "downloads", "duplicates", "applications", "artifacts", "node"].contains(store.selectedModuleID ?? "") {
                    HStack(spacing: Space.md) {
                        Picker("Size", selection: $store.sizeFilter) { ForEach(SizeFilter.allCases, id: \.self) { Text($0.rawValue).tag($0) } }
                        Picker("Modified", selection: $store.ageFilter) { ForEach(AgeFilter.allCases, id: \.self) { Text($0.rawValue).tag($0) } }
                    }.font(TypeStyle.caption)
                }
                HStack {
                    if store.isScanning { ProgressView().controlSize(.small) }
                    Text(store.progress).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1)
                    Spacer()
                }
                WarningView(store: store)
                if store.selectedModuleID == "storage", !store.visibleFindings.isEmpty {
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Largest items · logical size in this folder").font(TypeStyle.caption).foregroundStyle(.secondary)
                            ForEach(store.visibleFindings.sorted { ($0.bytes ?? 0) > ($1.bytes ?? 0) }.prefix(5)) { finding in
                                StorageBar(finding.title, value: Display.bytes(finding.bytes), fraction: store.diskBytes > 0 ? Double(finding.bytes ?? 0) / Double(store.diskBytes) : 0)
                            }
                        }
                    }
                }
            }.padding(Space.xl)
            Divider()
            if store.visibleFindings.isEmpty {
                EmptyState(store.isScanning ? "Looking for items…" : "No results to show", message: store.isScanning ? "Results appear as the scan progresses." : "Scan this tool or adjust your roots and search. Review warnings for incomplete coverage.", symbol: store.currentModule?.symbol ?? "tray")
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(store.visibleFindings) { finding in
                            ResultRow(title: finding.title, subtitle: (finding.subtitle as NSString).abbreviatingWithTildeInPath, value: Display.value(finding), badge: finding.blockedReason == nil ? finding.risk.rawValue : "Inspect", symbol: store.currentModule?.symbol ?? "doc", active: store.inspectedID == finding.id, eligible: !finding.actions.isEmpty && finding.blockedReason == nil && !store.isApplying, checked: Binding(get: { store.selectedIDs.contains(finding.id) }, set: { store.select(finding.id, checked: $0) })) {
                                store.inspectedID = finding.id
                            }
                            .contextMenu {
                                Button("Reveal in Finder") { store.reveal(finding) }
                                Button("Copy Path") { copy(finding.resource.path ?? finding.subtitle) }
                                if case .process(let identity) = finding.resource {
                                    Button("Copy PID") { copy(String(identity.pid)) }
                                    Button("Copy Command") { copy(finding.details.first { $0.label == "Command" }?.value ?? "") }
                                    Button("Ignore Process Name") { store.ignore(finding) }
                                } else { Button("Protect Path") { store.protect(finding) } }
                            }
                            Divider().padding(.leading, Space.lg)
                        }
                    }
                }
            }
            Divider()
            SelectionFooter(store: store)
        }
    }
    private var sortDirectionLabel: String {
        switch store.sort {
        case .name: store.sortAscending ? "Sorted A to Z; switch to Z to A" : "Sorted Z to A; switch to A to Z"
        case .size, .cpu: store.sortAscending ? "Smallest first; switch to largest first" : "Largest first; switch to smallest first"
        }
    }
    private func copy(_ value: String) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(value, forType: .string) }
}

/// Aggregate figures for the rows matching the current search and filters.
private struct SearchSummary: View {
    @Bindable var store: AppStore
    var body: some View {
        let rows = store.visibleFindings
        let summary = store.summary
        VStack(alignment: .leading, spacing: Space.xs) {
            if !store.search.isEmpty {
                Text("Matching “\(store.search)”").font(TypeStyle.caption).foregroundStyle(Palette.muted)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: Space.sm) {
                    StatChip(rows.count == 1 ? "Item" : "Items", value: rows.count.formatted())
                    if summary.processCount > 0 {
                        StatChip("Memory", value: Display.bytes(summary.processMemoryBytes))
                        if let busiest = rows.compactMap(\.cpuPercent).max() {
                            StatChip("Highest CPU", value: String(format: "%.1f%%", busiest))
                        }
                    } else {
                        StatChip("Total size", value: Display.bytes(summary.diskBytes))
                        StatChip("Ready to review", value: Display.bytes(summary.reclaimableBytes), emphasized: summary.reclaimableBytes > 0)
                        if let largest = rows.max(by: { ($0.bytes ?? 0) < ($1.bytes ?? 0) }), let bytes = largest.bytes, bytes > 0 {
                            StatChip("Largest · " + largest.title, value: Display.bytes(bytes))
                        }
                    }
                    if summary.blocked > 0 { StatChip("Needs inspection", value: summary.blocked.formatted()) }
                    if store.selectedIDs.count > 0 { StatChip("Selected", value: store.selectedIDs.count.formatted()) }
                }
            }
        }
        .task(id: store.summaryKey) {
            // Debounce typing and streaming results before asking the core for totals.
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled else { return }
            await store.refreshSummary()
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

private struct SelectionFooter: View {
    @Bindable var store: AppStore
    var body: some View {
        HStack(spacing: Space.md) {
            VStack(alignment: .leading, spacing: Space.xs) {
                Text("\(store.selectedIDs.count) selected").font(TypeStyle.sectionTitle)
                Text(summary).font(TypeStyle.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if !store.selectedIDs.isEmpty { ActionButton("Clear", disabled: store.isApplying) { store.selectedIDs.removeAll() } }
            ForEach(store.availableActions, id: \.self) { action in
                ActionButton("Review " + action.label, kind: action == .trash ? .primary : .destructive, disabled: store.isApplying || store.isScanning) { store.reviewSelection(action) }
            }
        }.padding(Space.lg)
    }
    private var summary: String {
        if store.selectedFindings.contains(where: { if case .process = $0.resource { true } else { false } }) { return "Process actions do not delete files or reclaim disk space." }
        if store.selectedIDs.isEmpty { return "Select items to review an action." }
        let bytes = store.core.normalizedSelection(store.selectedFindings).reduce(UInt64(0)) { $0 + ($1.bytes ?? 0) }
        return Display.bytes(bytes) + " logical size · review consequences before applying"
    }
}

private struct InspectorView: View {
    @Bindable var store: AppStore
    var body: some View {
        ScrollView {
            if let finding = store.inspected {
                VStack(alignment: .leading, spacing: Space.xl) {
                    PageHeader(finding.title, subtitle: finding.moduleID == "orphans" ? "Suspected orphan process" : "Item details")
                    StatusBadge(finding.risk.rawValue, warning: finding.risk == .permanent)
                    Text(finding.reason).font(TypeStyle.secondary)
                    if let blocked = finding.blockedReason { Label(blocked, systemImage: "lock").font(TypeStyle.secondary).foregroundStyle(Palette.warning) }
                    if let bytes = finding.bytes { KeyValueRow("Logical size", Display.bytes(bytes)) }
                    if let allocated = finding.allocatedBytes { KeyValueRow("Allocated size estimate", Display.bytes(allocated)) }
                    if let modified = finding.modifiedAt { KeyValueRow("Last modified", modified.formatted()) }
                    if let memory = finding.memoryBytes { KeyValueRow("Memory footprint", Display.bytes(memory)) }
                    if let path = finding.resource.path { KeyValueRow("Path", path) }
                    ForEach(Array(finding.details.enumerated()), id: \.offset) { _, detail in KeyValueRow(detail.label, detail.value) }
                    ActionButton("Reveal in Finder") { store.reveal(finding) }
                    if finding.moduleID == "storage", let path = finding.resource.path, store.core.isDirectory(path) {
                        ActionButton("Inspect folder", kind: .primary, disabled: store.isScanning || store.demo) { store.browse(path) }
                    }
                    if case .process = finding.resource { ActionButton("Ignore process name") { store.ignore(finding) } }
                    else { ActionButton("Protect this path") { store.protect(finding) } }
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
            PageHeader(draft.request.kind.label, subtitle: Display.items(draft.request.findings.count) + " selected for review")
            Panel { Label(draft.request.kind.consequence, systemImage: "exclamationmark.circle").font(TypeStyle.secondary).fixedSize(horizontal: false, vertical: true) }
            ScrollView {
                VStack(alignment: .leading, spacing: Space.lg) {
                    ForEach(draft.request.findings) { finding in
                        VStack(alignment: .leading, spacing: Space.xs) {
                            Text(finding.title).font(TypeStyle.sectionTitle)
                            Text(finding.resource.path ?? finding.subtitle).font(TypeStyle.code).textSelection(.enabled)
                            Text(finding.reason).font(TypeStyle.caption).foregroundStyle(.secondary)
                        }
                        Divider()
                    }
                }
            }
            HStack {
                if store.isApplying { ProgressView().controlSize(.small); Text("Applying reviewed actions…").font(TypeStyle.caption) }
                Spacer()
                ActionButton("Cancel", disabled: store.isApplying) { store.review = nil }
                ActionButton(draft.request.kind.label, kind: .destructive, disabled: store.isApplying || store.demo) { Task { await store.apply(draft) } }
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
                if !store.history.isEmpty { ActionButton("Clear history", disabled: store.isApplying || store.demo) { Task { await store.clearHistory() } } }
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
                                        ActionButton("Restore", disabled: store.demo || store.isApplying) { Task { await store.restore(row) } }
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
        VStack(alignment: .leading, spacing: Space.lg) {
            PageHeader("Settings", subtitle: "Choose scan roots, protect paths and enable modules.")
            ScrollView {
                VStack(alignment: .leading, spacing: Space.xl) {
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Scan roots").font(TypeStyle.sectionTitle)
                            ForEach(store.roots, id: \.self) { root in HStack { Text(root).font(TypeStyle.code); Spacer(); Button("Remove") { store.roots.removeAll { $0 == root }; store.storageNavigation = []; store.persist() } } }
                            ActionButton("Add folders") { store.addRoot() }
                        }
                    }
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Protected paths").font(TypeStyle.sectionTitle)
                            if store.exclusions.isEmpty { Text("Protect a finding from its inspector or context menu.").font(TypeStyle.secondary).foregroundStyle(.secondary) }
                            ForEach(store.exclusions, id: \.self) { path in HStack { Text(path).font(TypeStyle.code); Spacer(); Button("Unprotect") { store.exclusions.removeAll { $0 == path }; store.persist() } } }
                        }
                    }
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Ignored process names").font(TypeStyle.sectionTitle)
                            ForEach(store.ignoredNames, id: \.self) { name in HStack { Text(name).font(TypeStyle.code); Spacer(); Button("Show again") { store.ignoredNames.removeAll { $0 == name }; store.persist() } } }
                            HStack { TextField("Executable name", text: $ignoredName); Button("Ignore") { if !ignoredName.isEmpty && !store.ignoredNames.contains(ignoredName) { store.ignoredNames.append(ignoredName); store.persist(); ignoredName = "" } }.disabled(ignoredName.isEmpty) }
                        }
                    }
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Modules").font(TypeStyle.sectionTitle)
                            ForEach(store.modules, id: \.id) { module in
                                Toggle(module.name, isOn: Binding(get: { !store.disabledModules.contains(module.id) }, set: { enabled in
                                    store.disabledModules.removeAll { $0 == module.id }
                                    if !enabled { store.disabledModules.append(module.id) }
                                    if !enabled && store.selectedModuleID == module.id { store.selectedModuleID = "overview" }
                                    store.persist()
                                }))
                            }
                        }
                    }
                    Panel {
                        VStack(alignment: .leading, spacing: Space.md) {
                            Text("Appearance").font(TypeStyle.sectionTitle)
                            Picker("Palette", selection: $store.theme) {
                                ForEach(ComfyTheme.allCases) { Text($0.rawValue).tag($0) }
                            }.pickerStyle(.segmented)
                        }
                    }
                    Toggle("Show menu bar summary and monitor orphan processes", isOn: $store.menuBarEnabled)
                }
            }
            HStack { Spacer(); ActionButton("Done", kind: .primary) { store.showSettings = false } }
        }.padding(Space.xl).frame(width: Layout.reviewWidth, height: Layout.windowMinHeight)
    }
}
