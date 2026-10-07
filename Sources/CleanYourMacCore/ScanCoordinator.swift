import Foundation

public struct ScanCoordinator: Sendable {
    public let registry: ModuleRegistry
    public init(registry: ModuleRegistry) { self.registry = registry }
    public func scan(moduleIDs: [String], context: ScanContext, concurrency: Int = 3) -> AsyncThrowingStream<ScanEvent, Error> {
        let modules = registry.modules.filter { moduleIDs.contains($0.descriptor.id) }
        return AsyncThrowingStream { continuation in
            let task = Task.detached(priority: .utility) {
                await withTaskGroup(of: Void.self) { group in
                    var index = 0
                    func add(_ module: any ScanModule) {
                        group.addTask {
                            do {
                                for try await event in module.scan(in: context) {
                                    try Task.checkCancellation()
                                    switch event {
                                    case .finding: continuation.yield(event)
                                    case .progress(let message): continuation.yield(.progress(module.descriptor.name + " · " + message))
                                    case .warning(let warning): continuation.yield(.warning(module.descriptor.name + ": " + warning))
                                    }
                                }
                            } catch is CancellationError {}
                            catch { continuation.yield(.warning(module.descriptor.name + ": " + error.localizedDescription)) }
                        }
                    }
                    while index < min(max(concurrency, 1), modules.count) { add(modules[index]); index += 1 }
                    while await group.next() != nil {
                        if Task.isCancelled { group.cancelAll(); break }
                        if index < modules.count { add(modules[index]); index += 1 }
                    }
                }
                if Task.isCancelled { continuation.finish(throwing: CancellationError()) } else { continuation.finish() }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
