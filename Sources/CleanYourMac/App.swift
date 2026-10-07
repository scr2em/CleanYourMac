import AppKit
import CleanYourMacDesignSystem
import CleanYourMacUI
import SwiftUI

@main @MainActor struct CleanYourMacApp: App {
    @State private var store: AppStore
    init() {
        NSApplication.shared.setActivationPolicy(.regular)
        let fixture = CommandLine.arguments.contains("--demo")
        // `--demo --demo-rows 1000000` previews a million synthetic rows.
        let rows = CommandLine.arguments.firstIndex(of: "--demo-rows").flatMap { CommandLine.arguments.indices.contains($0 + 1) ? Int(CommandLine.arguments[$0 + 1]) : nil } ?? 0
        let initial = AppStore(demo: fixture, demoRows: rows)
        if fixture, let index = CommandLine.arguments.firstIndex(of: "--screen"), CommandLine.arguments.indices.contains(index + 1) {
            initial.selectedModuleID = CommandLine.arguments[index + 1]
            if initial.selectedModuleID == "node" { initial.inspectedID = "demo-node" }
        }
        if CommandLine.arguments.contains("--dark") { NSApplication.shared.appearance = NSAppearance(named: .darkAqua) }
        _store = State(initialValue: initial)
    }
    var body: some Scene {
        WindowGroup(store.demo ? "CleanYourMac Demo" : "CleanYourMac") { WorkspaceView(store: store) }
            .defaultSize(width: Layout.windowWidth, height: Layout.windowHeight)
            .commands {
                CommandGroup(replacing: .newItem) {}
                CommandGroup(after: .toolbar) {
                    Button("Scan Current Tool") { store.scan() }.keyboardShortcut("r")
                    Button("Component Gallery") { store.selectedModuleID = "gallery" }
                }
            }
        Settings { PreferencesView(store: store) }
        MenuBarExtra("CleanYourMac", systemImage: "leaf", isInserted: $store.menuBarEnabled) {
            Button("Open CleanYourMac") { NSApplication.shared.activate(ignoringOtherApps: true) }
            Text("\(store.count(for: "orphans").formatted()) orphan candidates")
            Text("\(Display.bytes(store.overview.diskBytes)) found across scanned tools")
            Button("Inspect Orphan Processes") { store.selectedModuleID = "orphans"; NSApplication.shared.activate(ignoringOtherApps: true) }
            Divider()
            Button("Quit") { NSApplication.shared.terminate(nil) }
        }
    }
}
