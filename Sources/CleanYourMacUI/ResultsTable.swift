import AppKit
import CleanYourMacCore
import CleanYourMacDesignSystem
import SwiftUI

/// A virtualized result list. `NSTableView` reuses a handful of row views however many rows
/// the query has, and rows are fetched from the core a page at a time as they scroll in.
struct ResultsTable: NSViewRepresentable {
    @Bindable var store: AppStore
    let symbol: String

    func makeCoordinator() -> Coordinator { Coordinator(store: store, symbol: symbol) }

    func makeNSView(context: Context) -> NSScrollView {
        let table = KeyboardTableView()
        let coordinator = context.coordinator
        table.onKey = { [weak coordinator] event in coordinator?.handle(event) ?? false }
        table.onSelectAll = { [weak coordinator] in coordinator?.store.selectAllResults() }
        table.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result")))
        table.headerView = nil
        table.style = .plain
        table.rowHeight = Layout.rowMinimum
        table.usesAutomaticRowHeights = false
        table.selectionHighlightStyle = .none
        table.focusRingType = .none
        table.intercellSpacing = .zero
        table.gridStyleMask = .solidHorizontalGridLineMask
        table.backgroundColor = .clear
        table.dataSource = context.coordinator
        table.delegate = context.coordinator
        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false
        context.coordinator.table = table
        return scroll
    }

    func updateNSView(_ view: NSScrollView, context: Context) {
        let coordinator = context.coordinator
        coordinator.store = store
        coordinator.symbol = symbol
        // Reading these registers observation, so a new snapshot reloads the table.
        let snapshot = "\(store.queryID)|\(store.resultTotal)"
        if snapshot != coordinator.snapshot {
            coordinator.snapshot = snapshot
            coordinator.table?.reloadData()
        }
    }

    @MainActor final class Coordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate {
        var store: AppStore
        var symbol: String
        var snapshot = ""
        weak var table: NSTableView?
        init(store: AppStore, symbol: String) { self.store = store; self.symbol = symbol }

        func numberOfRows(in tableView: NSTableView) -> Int { store.resultTotal }

        func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
            store.prefetch(row)
            let identifier = NSUserInterfaceItemIdentifier("ResultCell")
            let cell = tableView.makeView(withIdentifier: identifier, owner: nil) as? HostingCell ?? HostingCell(identifier: identifier)
            cell.show(ResultCell(store: store, index: row, symbol: symbol) { [weak tableView] in
                // Keyboard selection follows the row last clicked.
                tableView?.window?.makeFirstResponder(tableView)
            })
            return cell
        }

        /// Arrow keys move between rows (Shift extends the selection), Space toggles the
        /// current row and Escape clears the selection.
        func handle(_ event: NSEvent) -> Bool {
            let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            switch event.keyCode {
            case 125, 126:
                guard flags.isDisjoint(with: [.command, .option, .control]) else { return false }
                let delta = event.keyCode == 125 ? 1 : -1
                let extend = flags.contains(.shift)
                Task { @MainActor [weak self] in
                    guard let index = await self?.store.moveCursor(by: delta, extend: extend) else { return }
                    self?.table?.scrollRowToVisible(index)
                }
                return true
            case 49:
                guard flags.isDisjoint(with: [.command, .option, .control, .shift]) else { return false }
                store.toggleCursorRow()
                return true
            case 53:
                store.deselectAll()
                return true
            default:
                return false
            }
        }
    }
}

/// A table that hands keys and Edit > Select All to the result store.
final class KeyboardTableView: NSTableView {
    var onKey: (@MainActor (NSEvent) -> Bool)?
    var onSelectAll: (@MainActor () -> Void)?
    override var acceptsFirstResponder: Bool { true }
    override func keyDown(with event: NSEvent) {
        if onKey?(event) != true { super.keyDown(with: event) }
    }
    override func selectAll(_ sender: Any?) { onSelectAll?() }
}

/// A reusable table cell hosting one SwiftUI row.
final class HostingCell: NSTableCellView {
    private let host = NSHostingView(rootView: AnyView(EmptyView()))
    init(identifier: NSUserInterfaceItemIdentifier) {
        super.init(frame: .zero)
        self.identifier = identifier
        host.translatesAutoresizingMaskIntoConstraints = false
        addSubview(host)
        NSLayoutConstraint.activate([
            host.leadingAnchor.constraint(equalTo: leadingAnchor),
            host.trailingAnchor.constraint(equalTo: trailingAnchor),
            host.topAnchor.constraint(equalTo: topAnchor),
            host.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }
    @available(*, unavailable) required init?(coder: NSCoder) { nil }
    func show(_ view: some View) { host.rootView = AnyView(view) }
}

/// One row, drawn from the page cache; a quiet placeholder shows until its page arrives.
private struct ResultCell: View {
    @Bindable var store: AppStore
    let index: Int
    let symbol: String
    let focus: @MainActor () -> Void
    var body: some View {
        if let row = store.row(at: index) {
            FindingRow(
                title: row.title,
                subtitle: (row.subtitle as NSString).abbreviatingWithTildeInPath,
                value: Display.value(row),
                badge: [row.badge ?? (row.blocked ? "Inspect" : row.risk.rawValue), Display.lastUsed(row.lastUsedAt)].compactMap { $0 }.joined(separator: " · "),
                icon: Display.icon(moduleID: row.moduleID, path: row.path, brand: row.brand, symbol: symbol),
                active: store.inspectedID == row.id,
                eligible: row.eligible && !store.isApplying,
                checked: Binding(get: { store.selectedIDs.contains(row.id) }, set: { store.select(row.id, checked: $0) })
            ) { store.click(row, at: index, Self.click); focus() }
            .contextMenu {
                if row.eligible {
                    let selected = store.selectedIDs.contains(row.id)
                    Button(selected ? "Deselect" : "Select") { store.select(row.id, checked: !selected) }
                }
                Button("Select All Results") { store.selectAllResults() }
                if !store.selectedIDs.isEmpty { Button("Deselect All") { store.deselectAll() } }
                Divider()
                Button("Reveal in Finder") { store.reveal(id: row.id, path: row.path) }
                Button("Copy Path") { copy(row.path ?? row.subtitle) }
                if let pid = row.pid {
                    Button("Copy PID") { copy(String(pid)) }
                    Button("Ignore Process Name") { store.ignore(processName: row.title, id: row.id) }
                } else {
                    Button("Protect Path") { store.protect(path: row.path, id: row.id) }
                }
            }
        } else {
            HStack(spacing: Space.md) {
                RoundedRectangle(cornerRadius: Layout.smallRadius).fill(Palette.track).frame(width: Layout.checkbox, height: Layout.checkbox)
                RoundedRectangle(cornerRadius: Layout.smallRadius).fill(Palette.track).frame(height: Layout.iconSmall)
            }
            .padding(.horizontal, Space.lg)
            .frame(maxHeight: .infinity)
            .accessibilityLabel("Loading row")
        }
    }
    private func copy(_ value: String) { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(value, forType: .string) }
    /// The modifier held during the click that is being handled.
    private static var click: AppStore.Click {
        let flags = NSEvent.modifierFlags
        if flags.contains(.shift) { return .extend }
        if flags.contains(.command) { return .toggle }
        return .plain
    }
}
