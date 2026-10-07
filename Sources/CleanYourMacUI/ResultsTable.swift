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
        let table = NSTableView()
        table.addTableColumn(NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result")))
        table.headerView = nil
        table.style = .plain
        table.rowHeight = Layout.rowMinimum
        table.usesAutomaticRowHeights = false
        table.selectionHighlightStyle = .none
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
            cell.show(ResultCell(store: store, index: row, symbol: symbol))
            return cell
        }
    }
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
    var body: some View {
        if let row = store.row(at: index) {
            FindingRow(
                title: row.title,
                subtitle: (row.subtitle as NSString).abbreviatingWithTildeInPath,
                value: Display.value(row),
                badge: [row.badge ?? (row.blocked ? "Inspect" : row.risk.rawValue), Display.lastUsed(row.lastUsedAt)].compactMap { $0 }.joined(separator: " · "),
                symbol: symbol,
                active: store.inspectedID == row.id,
                eligible: row.eligible && !store.isApplying,
                checked: Binding(get: { store.selectedIDs.contains(row.id) }, set: { store.select(row.id, checked: $0) })
            ) { store.click(row, at: index, Self.click) }
            .contextMenu {
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
