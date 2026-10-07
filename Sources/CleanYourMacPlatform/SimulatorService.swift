import CleanYourMacCore
import Foundation

public struct SimulatorDevice: Decodable, Sendable {
    public let udid: String
    public let name: String
    public let state: String
    public let isAvailable: Bool?
    public let dataPath: String?
}
public struct SimulatorRuntime: Decodable, Sendable {
    public let identifier: String
    public let name: String
    public let version: String?
    public let isAvailable: Bool?
    public let bundlePath: String?
}
public struct SimulatorInventory: Decodable, Sendable {
    public let devices: [String: [SimulatorDevice]]
    public let runtimes: [SimulatorRuntime]
}

public struct SimulatorService: Sendable {
    private let runner: any CommandRunning
    public init(runner: any CommandRunning = CommandRunner()) { self.runner = runner }
    public func inventory() async throws -> SimulatorInventory {
        let result = try await runner.run("/usr/bin/xcrun", ["simctl", "list", "--json"], timeout: 20)
        guard result.status == 0 else { throw CleanError.message("Xcode simulator service unavailable. Open Xcode once and check its selected developer directory. \(result.error.prefix(200))") }
        return try JSONDecoder().decode(SimulatorInventory.self, from: result.data)
    }
    public func act(id: String, action: ActionKind) async throws {
        guard UUID(uuidString: id) != nil else { throw CleanError.message("Invalid simulator identifier.") }
        let inventory = try await inventory()
        guard let device = inventory.devices.values.flatMap({ $0 }).first(where: { $0.udid == id }), device.state == "Shutdown" else { throw CleanError.message("The selected simulator no longer exists or is running. Refresh first.") }
        let verb: String
        switch action {
        case .resetSimulator: verb = "erase"
        case .deleteSimulator: verb = "delete"
        default: throw CleanError.message("Unsupported simulator action.")
        }
        let result = try await runner.run("/usr/bin/xcrun", ["simctl", verb, id], timeout: 45)
        guard result.status == 0 else { throw CleanError.message(result.error) }
    }
}
