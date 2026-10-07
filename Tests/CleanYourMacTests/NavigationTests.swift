import CleanYourMacUI
import Foundation
import Testing

@MainActor @Test func unscannedToolNeverInheritsAnotherToolsCompletion() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    _ = try fixture.write("app/package.json", "{}")
    _ = try fixture.write("app/node_modules/fixture/index.js")
    let defaults = try #require(UserDefaults(suiteName: "org.cleanyourmac.test." + UUID().uuidString))
    let store = AppStore(defaults: defaults)
    store.roots = [fixture.path]
    store.selectedModuleID = "node"
    store.scan()
    for _ in 0..<200 where store.isScanning { try await Task.sleep(for: .milliseconds(10)) }
    #expect(!store.isScanning)
    #expect(store.progress == "Scan complete")
    #expect(store.visibleFindings.count == 1)
    store.selectedIDs = Set(store.visibleFindings.map(\.id))
    store.selectedModuleID = "simulators"
    #expect(store.progress == "Ready · scan this tool")
    #expect(store.selectedIDs.isEmpty)
    #expect(store.visibleFindings.isEmpty)
    store.selectedModuleID = "node"
    #expect(store.progress == "Scan complete")
    #expect(store.visibleFindings.count == 1)
}
