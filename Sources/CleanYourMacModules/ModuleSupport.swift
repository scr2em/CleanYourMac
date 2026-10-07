import CleanYourMacCore
import CleanYourMacPlatform
import Foundation

enum Streams {
    static func make(_ work: @escaping @Sendable (AsyncThrowingStream<ScanEvent, Error>.Continuation) async throws -> Void) -> AsyncThrowingStream<ScanEvent, Error> {
        AsyncThrowingStream { continuation in
            let task = Task.detached(priority: .utility) {
                do { try await work(continuation); continuation.finish() }
                catch is CancellationError { continuation.finish(throwing: CancellationError()) }
                catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
    static func file(_ entry: FileEntry, module: String, title: String? = nil, reason: String, actions: [ActionKind] = [.trash], risk: Risk = .review, details: [Detail] = []) throws -> Finding {
        let files = FileService(), size = try files.size(entry.path)
        let fresh = try files.entry(entry.path)
        let protected = PathPolicy.protected(entry.path)
        return Finding(id: module + ":" + entry.path, moduleID: module, title: title ?? (entry.path as NSString).lastPathComponent, subtitle: entry.path, resource: .file(fresh.identity.withTreeSignature(size.signature)), bytes: size.logical, allocatedBytes: size.allocated, modifiedAt: entry.modified, details: details + [Detail("Modified", entry.modified.formatted()), Detail("Files", String(size.files)), Detail("Size coverage", size.complete ? "Complete" : "Partial estimate")], actions: size.complete && !protected ? actions : [], risk: risk, reason: reason, blockedReason: protected ? "Protected path; inspect manually." : size.complete ? nil : "Incomplete size/permission coverage. Inspect manually.")
    }
    static func children(_ root: String, continuation: AsyncThrowingStream<ScanEvent, Error>.Continuation) -> [FileEntry] {
        do {
            return try FileManager.default.contentsOfDirectory(atPath: root).compactMap { name in
                let path = root + "/" + name
                do { return try FileService().entry(path) }
                catch { continuation.yield(.warning("Skipped \(path): \(error.localizedDescription)")); return nil }
            }
        } catch { continuation.yield(.warning("Cannot read \(root): \(error.localizedDescription)")); return [] }
    }
}

public enum BuiltInModules {
    public static func registry(processes: ProcessService) -> ModuleRegistry {
        ModuleRegistry([
            StorageModule(), LargeFilesModule(), DuplicateModule(), NodeModule(), ArtifactModule(),
            WorktreeModule(), SimulatorModule(), XcodeModule(), CacheModule(), ApplicationsModule(), AppLeftoversModule(),
            DownloadsModule(), TrashModule(), OrphanModule(processes: processes),
        ])
    }
}
