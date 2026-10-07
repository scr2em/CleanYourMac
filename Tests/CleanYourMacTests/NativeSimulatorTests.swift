import CleanYourMacCore
import CleanYourMacPlatform
import Foundation
import Testing

@Test(.enabled(if: ProcessInfo.processInfo.environment["CYM_NATIVE_SIMULATOR_TESTS"] == "1"))
func nativeSimulatorActionsUseOnlyNewDisposableDevice() async throws {
    let runner = CommandRunner(), service = SimulatorService()
    let inventory = try await service.inventory()
    let runtime = try #require(inventory.runtimes.first { $0.identifier.contains(".iOS-") && $0.isAvailable == true })
    let typesOutput = try await runner.run("/usr/bin/xcrun", ["simctl", "list", "devicetypes", "--json"], timeout: 20)
    struct DeviceType: Decodable { let identifier: String; let name: String }
    struct Types: Decodable { let devicetypes: [DeviceType] }
    let types = try JSONDecoder().decode(Types.self, from: typesOutput.data)
    let type = try #require(types.devicetypes.first { $0.name == "iPhone SE (3rd generation)" })
    let created = try await runner.run("/usr/bin/xcrun", ["simctl", "create", "CleanYourMac Disposable Fixture " + UUID().uuidString, type.identifier, runtime.identifier], timeout: 30)
    #expect(created.status == 0)
    let id = created.text.trimmingCharacters(in: .whitespacesAndNewlines)
    #expect(UUID(uuidString: id) != nil)
    guard UUID(uuidString: id) != nil else { throw CleanError.message("Fixture simulator could not be created.") }
    do {
        #expect(try await service.inventory().devices.values.flatMap { $0 }.contains { $0.udid == id && $0.state == "Shutdown" })
        try await service.act(id: id, action: .resetSimulator)
        try await service.act(id: id, action: .deleteSimulator)
        #expect(try await !service.inventory().devices.values.flatMap { $0 }.contains { $0.udid == id })
    } catch {
        // The UUID came only from this test's create result, never from existing devices.
        try? await service.act(id: id, action: .deleteSimulator)
        throw error
    }
}
