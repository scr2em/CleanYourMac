import CleanYourMacCore
import Darwin
import Foundation

public struct CommandOutput: Sendable {
    public let data: Data
    public let error: String
    public let status: Int32
    public var text: String { String(decoding: data, as: UTF8.self) }
    public init(data: Data, error: String, status: Int32) { self.data = data; self.error = error; self.status = status }
}

public protocol CommandRunning: Sendable {
    func run(_ executable: String, _ arguments: [String], timeout: TimeInterval) async throws -> CommandOutput
}

public struct CommandRunner: CommandRunning {
    public init() {}
    public func run(_ executable: String, _ arguments: [String], timeout: TimeInterval = 15) async throws -> CommandOutput {
        let allowed = ["/usr/bin/git", "/usr/bin/xcrun", "/bin/launchctl"]
        guard allowed.contains(executable) else { throw CleanError.message("Executable is not an approved platform adapter.") }
        let session = CommandSession(executable: executable, arguments: arguments, timeout: timeout)
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { session.start($0) }
        } onCancel: { session.cancel() }
    }
}

private final class CommandSession: @unchecked Sendable {
    private let process = Process()
    private let output = Pipe(), errors = Pipe()
    private let lock = NSLock()
    private var cancelled = false
    private var continuation: CheckedContinuation<CommandOutput, any Error>?
    private var out = Data(), err = Data()
    private var failure: String?
    private let timeout: TimeInterval
    private let maximum = 4 * 1024 * 1024

    init(executable: String, arguments: [String], timeout: TimeInterval) {
        process.executableURL = URL(fileURLWithPath: executable); process.arguments = arguments
        process.standardOutput = output; process.standardError = errors; process.standardInput = FileHandle.nullDevice
        process.environment = [
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin",
            "HOME": NSHomeDirectory(), "LC_ALL": "C", "LANG": "C", "GIT_OPTIONAL_LOCKS": "0",
        ]
        self.timeout = timeout
    }

    func start(_ continuation: CheckedContinuation<CommandOutput, any Error>) {
        lock.lock()
        self.continuation = continuation
        let alreadyCancelled = cancelled
        lock.unlock()
        guard !alreadyCancelled else { finish(error: CancellationError()); return }
        let group = DispatchGroup()
        group.enter(); group.enter()
        process.terminationHandler = { [self] _ in
            group.notify(queue: .global(qos: .utility)) { self.completed() }
        }
        do { try process.run() } catch { finish(error: error); return }
        // Only the child owns the write ends; retaining them here prevents EOF.
        try? output.fileHandleForWriting.close()
        try? errors.fileHandleForWriting.close()
        // Drain both pipes concurrently to avoid deadlocks on a full stderr pipe.
        DispatchQueue.global(qos: .utility).async { self.read(self.output.fileHandleForReading, isError: false); group.leave() }
        DispatchQueue.global(qos: .utility).async { self.read(self.errors.fileHandleForReading, isError: true); group.leave() }
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeout) { [weak self] in self?.stop(reason: "Tool timed out.") }
        lock.lock(); let shouldCancel = cancelled; lock.unlock()
        if shouldCancel { stop(reason: nil) }
    }

    private func completed() {
        lock.lock()
        let failure = failure, cancelled = cancelled
        let result = CommandOutput(data: out, error: String(decoding: err, as: UTF8.self), status: process.terminationStatus)
        lock.unlock()
        if cancelled { finish(error: CancellationError()) }
        else if let failure { finish(error: CleanError.message(failure)) }
        else { finish(output: result) }
    }

    private func read(_ handle: FileHandle, isError: Bool) {
        while let chunk = try? handle.read(upToCount: 16_384), !chunk.isEmpty {
            lock.lock()
            let overflow = out.count + err.count + chunk.count > maximum
            if !overflow {
                if isError { err.append(chunk) } else { out.append(chunk) }
            }
            lock.unlock()
            if overflow { stop(reason: "Tool output exceeded the inspection limit."); break }
        }
    }

    func cancel() {
        lock.lock(); cancelled = true; lock.unlock()
        stop(reason: nil)
    }
    private func stop(reason: String?) {
        lock.lock()
        if continuation == nil { lock.unlock(); return }
        if let reason { failure = reason }
        lock.unlock()
        if process.isRunning {
            process.terminate()
            let ownedProcess = process
            DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 1) {
                guard ownedProcess.isRunning else { return }
                _ = kill(ownedProcess.processIdentifier, SIGKILL)
            }
        }
        try? output.fileHandleForReading.close()
        try? errors.fileHandleForReading.close()
        // Bound completion even if a tool's descendant inherits its pipe handles.
        finish(error: reason.map { CleanError.message($0) } ?? CancellationError())
    }
    private func finish(output: CommandOutput? = nil, error: (any Error)? = nil) {
        lock.lock(); let callback = continuation; continuation = nil; lock.unlock()
        guard let callback else { return }
        process.terminationHandler = nil
        if let error { callback.resume(throwing: error) }
        else if let output { callback.resume(returning: output) }
    }
}
