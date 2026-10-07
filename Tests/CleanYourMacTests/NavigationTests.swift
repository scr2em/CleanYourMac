import CleanYourMacCore
import CleanYourMacUI
import Foundation
import Testing

@MainActor private func waitForScan(_ store: AppStore) async throws {
    for _ in 0..<500 where store.isScanning { try await Task.sleep(for: .milliseconds(10)) }
}

@MainActor @Test func unscannedToolNeverInheritsAnotherToolsCompletion() async throws {
    let fixture = try Fixture(); defer { fixture.clean() }
    try fixture.write("app/package.json", "{}")
    try fixture.write("app/node_modules/fixture/index.js")
    let defaults = try #require(UserDefaults(suiteName: "org.cleanyourmac.test." + UUID().uuidString))
    let store = AppStore(defaults: defaults, core: CoreEngine(journalPath: fixture.path + "/history.json"))
    store.roots = [fixture.path]
    store.selectedModuleID = "node"
    store.scan()
    try await waitForScan(store)
    #expect(!store.isScanning)
    #expect(store.progress == "Scan complete")
    await store.requery()
    #expect(store.resultTotal == 1)
    let row = try #require(await store.rows(0..<1).first)
    #expect(row.moduleID == "node")
    store.selectedIDs = [row.id]
    store.selectedModuleID = "simulators"
    #expect(store.progress == "Ready · scan this tool")
    #expect(store.selectedIDs.isEmpty)
    await store.requery()
    #expect(store.resultTotal == 0)
    store.selectedModuleID = "node"
    #expect(store.progress == "Scan complete")
    await store.requery()
    #expect(store.resultTotal == 1)
}

@MainActor @Test func sortDirectionAndSearchSummaryFollowTheQuery() async throws {
    let defaults = try #require(UserDefaults(suiteName: "org.cleanyourmac.test." + UUID().uuidString))
    let store = AppStore(demo: true, defaults: defaults, core: CoreEngine())
    #expect(store.selectedModuleID == "node")
    await store.requery()
    #expect(await store.rows(0..<2).map(\.id) == ["demo-node", "demo-node-2"])
    store.sortAscending = true
    await store.requery()
    #expect(await store.rows(0..<2).map(\.id) == ["demo-node-2", "demo-node"])
    store.sort = .name
    #expect(store.sortAscending)
    await store.requery()
    #expect(await store.rows(0..<2).map(\.title) == ["dashboard", "landing-page"])
    store.search = "landing"
    await store.requery()
    #expect(store.resultTotal == 1)
    #expect(store.summary.diskBytes == 864_000_000)
    #expect(store.summary.reclaimableBytes == 864_000_000)
    store.selectedIDs = ["demo-node", "demo-node-2"]
    await store.refreshSelection()
    #expect(store.selection.count == 2)
    #expect(store.availableActions == [.trash])
}

@MainActor @Test func pagesLoadOnDemandForLargeResultSets() async throws {
    let defaults = try #require(UserDefaults(suiteName: "org.cleanyourmac.test." + UUID().uuidString))
    let store = AppStore(demo: true, demoRows: 50_000, defaults: defaults, core: CoreEngine())
    #expect(store.selectedModuleID == "large")
    await store.requery()
    #expect(store.resultTotal == 10_000)
    #expect(store.row(at: 9_999) == nil)
    store.prefetch(9_999)
    for _ in 0..<200 where store.row(at: 9_999) == nil { try await Task.sleep(for: .milliseconds(10)) }
    #expect(store.row(at: 9_999) != nil)
    #expect(store.row(at: 0) == nil)
    store.selectedModuleID = "overview"
    await store.requery()
    #expect(store.resultTotal == 50_000)
    #expect(store.overview.findings == 50_000)
}

@MainActor @Test func commandClickTogglesAndShiftClickSelectsAnEligibleRange() async throws {
    let defaults = try #require(UserDefaults(suiteName: "org.cleanyourmac.test." + UUID().uuidString))
    let store = AppStore(demo: true, demoRows: 5_000, defaults: defaults, core: CoreEngine())
    await store.requery()
    let rows = await store.rows(0..<40)
    let eligible = rows.filter(\.eligible)
    let first = try #require(eligible.first), last = try #require(eligible.last)
    let firstIndex = try #require(rows.firstIndex(of: first)), lastIndex = try #require(rows.firstIndex(of: last))

    store.click(first, at: firstIndex, .plain)
    #expect(store.inspectedID == first.id)
    #expect(store.selectedIDs.isEmpty)

    store.click(first, at: firstIndex, .toggle)
    #expect(store.selectedIDs == [first.id])

    store.click(last, at: lastIndex, .extend)
    for _ in 0..<200 where store.selectedIDs.count < 2 { try await Task.sleep(for: .milliseconds(10)) }
    let range = Set(rows[firstIndex...lastIndex].filter(\.eligible).map(\.id))
    #expect(store.selectedIDs == range)

    store.click(first, at: firstIndex, .toggle)
    #expect(!store.selectedIDs.contains(first.id))
    if let blocked = rows.first(where: { !$0.eligible }) {
        store.click(blocked, at: rows.firstIndex(of: blocked)!, .toggle)
        #expect(!store.selectedIDs.contains(blocked.id))
    }
}
