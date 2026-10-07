import CleanYourMacCore
import CleanYourMacPlatform
import Darwin
import Foundation
import Testing

@Test(.enabled(if: ProcessInfo.processInfo.environment["CYM_NATIVE_PROCESS_TESTS"] == "1"))
func nativeOrphanTerminationAndExplicitForceQuit() async throws {
    let executable = FileManager.default.currentDirectoryPath + "/.build/NativeFixtures/OrphanFixture"
    #expect(FileManager.default.isExecutableFile(atPath: executable))
    let service = ProcessService()
    for ignoreTerm in [false, true] {
        let child = Process(), pipe = Pipe()
        child.executableURL = URL(fileURLWithPath: executable)
        child.arguments = ignoreTerm ? ["ignore-term"] : []
        child.standardOutput = pipe; child.standardError = FileHandle.nullDevice
        try child.run()
        try pipe.fileHandleForWriting.close()
        let data = try pipe.fileHandleForReading.readToEnd() ?? Data()
        let pid = try #require(Int32(String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)))
        try await Task.sleep(for: .milliseconds(100))
        let identity = try #require(ProcessService.inspect(pid)?.identity)
        defer {
            if ProcessService.inspect(pid)?.identity == identity { _ = kill(pid, SIGKILL) }
        }
        #expect(identity.executable == executable)
        let candidates = try await service.candidates(ignoring: [])
        #expect(candidates.contains { $0.0.identity == identity })
        let result = try await service.signal(identity, force: false, ignoring: [])
        #expect(result == (ignoreTerm ? .requested : .applied))
        if ignoreTerm { #expect(try await service.signal(identity, force: true, ignoring: []) == .applied) }
        #expect(ProcessService.inspect(pid)?.identity != identity)
    }
}
