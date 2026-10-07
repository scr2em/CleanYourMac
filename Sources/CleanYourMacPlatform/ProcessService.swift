// Process inspection adapted from OrphanBar; see THIRD_PARTY_NOTICES.md.
import AppKit
import CleanYourMacCore
import Darwin
import Foundation

public struct ProcessSnapshot: Sendable {
    public let identity: ProcessIdentity
    public let parent: Int32
    public let name: String
    public let command: String
    public let workingDirectory: String?
    public let cpuNanos: UInt64?
    public let memoryBytes: UInt64?
    public init(identity: ProcessIdentity, parent: Int32, name: String, command: String, workingDirectory: String?, cpuNanos: UInt64?, memoryBytes: UInt64?) {
        self.identity = identity; self.parent = parent; self.name = name; self.command = command; self.workingDirectory = workingDirectory; self.cpuNanos = cpuNanos; self.memoryBytes = memoryBytes
    }
}

public enum OrphanClassifier {
    public static func exclusion(
        process: ProcessSnapshot, uid: UInt32, ownPID: Int32, managed: Set<Int32>,
        appPIDs: Set<Int32>, appBundles: Set<String>, ignored: Set<String>
    ) -> String? {
        if process.identity.uid != uid { return "Another user's process" }
        if process.identity.pid == ownPID || process.identity.pid <= 1 { return "App or launchd" }
        if process.parent != 1 { return "Has a living parent" }
        if managed.contains(process.identity.pid) { return "Managed launchd job" }
        if appPIDs.contains(process.identity.pid) { return "Running application" }
        if ignored.contains(process.name) { return "Ignored process name" }
        let path = process.identity.executable
        let protected = ["/System/", "/usr/libexec/", "/usr/sbin/", "/sbin/", "/Library/Apple/", "/Library/Developer/PrivateFrameworks/", "/Library/Developer/CoreSimulator/", "/Library/Filesystems/", "/Library/PrivilegedHelperTools/"]
        if protected.contains(where: { path.hasPrefix($0) }) || path.contains(".xpc/") || path.contains(".appex/") { return "System component or service" }
        if let range = path.range(of: ".app/"), appBundles.contains(String(path[..<range.lowerBound]) + ".app") { return "Helper of a running app" }
        return nil
    }
}

public actor ProcessService {
    private struct Sample { let cpu: UInt64; let time: UInt64 }
    private var samples: [ProcessIdentity: Sample] = [:]
    private var terminationRequests: [ProcessIdentity: UInt64] = [:]
    private let runner: any CommandRunning
    public init(runner: any CommandRunning = CommandRunner()) { self.runner = runner }

    public func candidates(ignoring names: Set<String>) async throws -> [(ProcessSnapshot, Double?)] {
        let jobs = try await managedJobs()
        let apps = await runningApps()
        let now = DispatchTime.now().uptimeNanoseconds
        var next: [ProcessIdentity: Sample] = [:]
        var results: [(ProcessSnapshot, Double?)] = []
        for pid in Self.pids() {
            try Task.checkCancellation()
            guard let snapshot = Self.inspect(pid),
                  OrphanClassifier.exclusion(process: snapshot, uid: getuid(), ownPID: getpid(), managed: jobs, appPIDs: apps.0, appBundles: apps.1, ignored: names) == nil else { continue }
            var percent: Double?
            if let cpu = snapshot.cpuNanos {
                if let last = samples[snapshot.identity], now > last.time, cpu >= last.cpu {
                    percent = Double(cpu - last.cpu) / Double(now - last.time) * 100
                }
                next[snapshot.identity] = Sample(cpu: cpu, time: now)
            }
            results.append((snapshot, percent))
        }
        samples = next
        let candidateIdentities = Set(results.map { $0.0.identity })
        terminationRequests = terminationRequests.filter { candidateIdentities.contains($0.key) }
        return results.sorted { ($0.1 ?? 0, $0.0.memoryBytes ?? 0) > ($1.1 ?? 0, $1.0.memoryBytes ?? 0) }
    }

    public func signal(_ identity: ProcessIdentity, force: Bool, ignoring: Set<String>) async throws -> Outcome {
        if force {
            guard let requested = terminationRequests[identity], DispatchTime.now().uptimeNanoseconds >= requested + 2_000_000_000 else {
                throw CleanError.message("Request graceful termination first; Force Quit is available after two seconds if the process remains alive.")
            }
        }
        let jobs = try await managedJobs()
        let apps = await runningApps()
        guard let current = Self.inspect(identity.pid) else { return .skipped }
        guard current.identity == identity else { throw CleanError.message("PID now belongs to a different process. Refresh before acting.") }
        guard OrphanClassifier.exclusion(process: current, uid: getuid(), ownPID: getpid(), managed: jobs, appPIDs: apps.0, appBundles: apps.1, ignored: ignoring) == nil else { throw CleanError.message("Process no longer meets the orphan rules.") }
        guard kill(identity.pid, force ? SIGKILL : SIGTERM) == 0 else { throw CleanError.message(String(cString: strerror(errno))) }
        if !force { terminationRequests[identity] = DispatchTime.now().uptimeNanoseconds }
        // A delivered signal is not proof of exit. Observe the process instance separately.
        for _ in 0..<10 {
            try await Task.sleep(for: .milliseconds(200))
            guard let current = Self.inspect(identity.pid), current.identity == identity else { terminationRequests.removeValue(forKey: identity); return .applied }
        }
        return .requested
    }
    public static func activeTools(under directory: String? = nil) -> [String] {
        let names: Set<String> = ["node", "npm", "pnpm", "yarn", "bun", "swift", "swift-frontend", "xcodebuild", "clang", "cargo", "rustc", "python", "python3", "pip", "uv", "java", "gradle", "brew"]
        return pids().compactMap { pid in
            guard let process = inspect(pid), process.identity.uid == getuid(), names.contains(process.name.lowercased()) else { return nil }
            if let directory {
                guard let cwd = process.workingDirectory, PathPolicy.contains(cwd, in: directory) else { return nil }
            }
            return "\(process.name) (PID \(pid))"
        }
    }

    private func managedJobs() async throws -> Set<Int32> {
        let result = try await runner.run("/bin/launchctl", ["list"], timeout: 5)
        guard result.status == 0, result.text.hasPrefix("PID") else { throw CleanError.message("Managed-service inspection failed; orphan classification is incomplete and termination is unavailable.") }
        return Set(result.text.split(separator: "\n").dropFirst().compactMap { $0.split(whereSeparator: \.isWhitespace).first.flatMap { Int32($0) } })
    }
    private func runningApps() async -> (Set<Int32>, Set<String>) {
        await MainActor.run {
            let apps = NSWorkspace.shared.runningApplications
            return (Set(apps.map(\.processIdentifier)), Set(apps.compactMap { $0.bundleURL?.standardizedFileURL.path }))
        }
    }
    public static func inspect(_ pid: Int32) -> ProcessSnapshot? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        var pathBuffer = [UInt8](repeating: 0, count: 4 * Int(MAXPATHLEN))
        let length = pathBuffer.withUnsafeMutableBytes { proc_pidpath(pid, $0.baseAddress, UInt32($0.count)) }
        guard length > 0 else { return nil }
        let path = String(decoding: pathBuffer.prefix { $0 != 0 }, as: UTF8.self)
        let identity = ProcessIdentity(pid: pid, uid: info.pbi_uid, startedSeconds: info.pbi_start_tvsec, startedMicroseconds: info.pbi_start_tvusec, executable: path)
        var usage = rusage_info_v4()
        let available = withUnsafeMutablePointer(to: &usage) { pointer in
            pointer.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V4, $0) == 0 }
        }
        var timebase = mach_timebase_info_data_t()
        mach_timebase_info(&timebase)
        let ticks = usage.ri_user_time &+ usage.ri_system_time
        let denominator = UInt64(max(timebase.denom, 1)), numerator = UInt64(timebase.numer)
        let nanos = ticks / denominator * numerator + ticks % denominator * numerator / denominator
        let arguments = arguments(pid)
        return ProcessSnapshot(identity: identity, parent: Int32(info.pbi_ppid), name: (path as NSString).lastPathComponent, command: arguments.isEmpty ? path : arguments.joined(separator: " "), workingDirectory: workingDirectory(pid), cpuNanos: available ? nanos : nil, memoryBytes: available ? usage.ri_phys_footprint : nil)
    }
    private static func pids() -> [Int32] {
        let count = proc_listallpids(nil, 0)
        guard count > 0 else { return [] }
        var buffer = [Int32](repeating: 0, count: Int(count) + 256)
        let actual = buffer.withUnsafeMutableBytes { proc_listallpids($0.baseAddress, Int32($0.count)) }
        return actual > 0 ? Array(buffer.prefix(min(Int(actual), buffer.count))) : []
    }
    private static func workingDirectory(_ pid: Int32) -> String? {
        var info = proc_vnodepathinfo()
        let size = Int32(MemoryLayout<proc_vnodepathinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, size) == size else { return nil }
        return withUnsafeBytes(of: &info.pvi_cdir.vip_path) { raw in
            let bytes = raw.prefix { $0 != 0 }
            return bytes.isEmpty ? nil : String(decoding: bytes, as: UTF8.self)
        }
    }
    private static func arguments(_ pid: Int32) -> [String] {
        var mib: [Int32] = [CTL_KERN, KERN_ARGMAX], capacity: Int32 = 0
        var size = MemoryLayout<Int32>.size
        guard sysctl(&mib, 2, &capacity, &size, nil, 0) == 0, capacity > 0, capacity < 4_194_304 else { return [] }
        var buffer = [UInt8](repeating: 0, count: Int(capacity))
        mib = [CTL_KERN, KERN_PROCARGS2, pid]; size = buffer.count
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0, size > MemoryLayout<Int32>.size else { return [] }
        let argc = buffer.withUnsafeBytes { $0.loadUnaligned(as: Int32.self) }
        var index = MemoryLayout<Int32>.size
        while index < size, buffer[index] != 0 { index += 1 }
        while index < size, buffer[index] == 0 { index += 1 }
        var result: [String] = []
        while result.count < argc, index < size {
            let start = index
            while index < size, buffer[index] != 0 { index += 1 }
            result.append(String(decoding: buffer[start..<index], as: UTF8.self)); index += 1
        }
        return result
    }
}
