import CCymCore
import Foundation

/// Swift face of the Rust core. Requests and results cross the versioned C ABI as JSON.
/// The engine is thread-safe; blocking calls that may take seconds are exposed as async.
public final class CoreEngine: @unchecked Sendable {
    public static let shared = CoreEngine()
    private let handle: OpaquePointer

    /// - Parameter journalPath: action history location; defaults to the user's Application Support.
    public init(journalPath: String? = nil) {
        precondition(cym_abi_version() == UInt32(CYM_ABI_VERSION), "CleanYourMac core ABI mismatch")
        var config = ""
        if let journalPath, let data = try? Self.encoder.encode(["journalPath": journalPath]) { config = String(decoding: data, as: UTF8.self) }
        guard let handle = config.withCString({ cym_engine_new($0) }) else { preconditionFailure("Core engine configuration was rejected") }
        self.handle = handle
    }
    deinit { cym_engine_free(handle) }

    static let encoder: JSONEncoder = {
        let encoder = JSONEncoder(); encoder.dateEncodingStrategy = .secondsSince1970; return encoder
    }()
    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder(); decoder.dateDecodingStrategy = .secondsSince1970; return decoder
    }()

    private struct Request<Params: Encodable>: Encodable { let method: String; let params: Params }
    private struct Response<Result: Decodable>: Decodable { let ok: Bool; let result: Result?; let error: String? }
    private struct Empty: Codable {}

    private func call<Params: Encodable, Result: Decodable>(_ method: String, _ params: Params, as: Result.Type = Result.self) throws -> Result? {
        let request = String(decoding: try Self.encoder.encode(Request(method: method, params: params)), as: UTF8.self)
        guard let raw = request.withCString({ cym_call(handle, $0) }) else { throw CleanError.message("The core returned no response.") }
        defer { cym_string_free(raw) }
        let response = try Self.decoder.decode(Response<Result>.self, from: Data(String(cString: raw).utf8))
        guard response.ok else { throw CleanError.message(response.error ?? "The core reported an unknown error.") }
        return response.result
    }
    private func value<Params: Encodable, Result: Decodable>(_ method: String, _ params: Params, as: Result.Type = Result.self) throws -> Result {
        guard let result = try call(method, params, as: Result.self) else { throw CleanError.message("The core returned an empty \(method) result.") }
        return result
    }
    private func detached<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
        try await Task.detached(priority: .userInitiated, operation: work).value
    }

    public func modules() -> [ModuleDescriptor] { (try? value("modules", Empty(), as: [ModuleDescriptor].self)) ?? [] }
    public func projectRoots() -> [String] { (try? value("projectRoots", Empty(), as: [String].self)) ?? [] }
    public func normalizeRoots(_ paths: [String]) -> [String] { (try? value("normalizeRoots", ["paths": paths], as: [String].self)) ?? paths }
    public func contains(_ path: String, in root: String) -> Bool { (try? value("pathContains", ["path": path, "root": root], as: Bool.self)) ?? false }
    public func isDirectory(_ path: String) -> Bool { (try? value("isDirectory", ["path": path], as: Bool.self)) ?? false }
    public func normalizedSelection(_ findings: [Finding]) -> [Finding] { (try? value("normalizedSelection", ["findings": findings], as: [Finding].self)) ?? [] }
    public func analytics(_ findings: [Finding]) async -> Analytics {
        (try? await detached { try self.value("analytics", ["findings": findings], as: Analytics.self) }) ?? .empty
    }
    public func execute(_ request: ActionRequest) async throws -> [ActionResult] {
        try await detached { try self.value("execute", ["request": request], as: [ActionResult].self) }
    }
    public func history() async -> [ActionResult] { (try? await detached { try self.value("history", Empty(), as: [ActionResult].self) }) ?? [] }
    public func clearHistory() async throws { _ = try await detached { try self.call("clearHistory", Empty(), as: Empty.self) } }
    public func restore(_ result: ActionResult) async throws {
        _ = try await detached { try self.call("restore", ["result": result], as: Empty.self) }
    }

    // Result store: scans write here; the app queries snapshots and reads them a page at a time.
    public func query(_ query: ResultQuery) async throws -> QueryInfo {
        try await detached { try self.value("query", ["query": query], as: QueryInfo.self) }
    }
    public func rows(queryID: UInt64, offset: Int, limit: Int) async throws -> [ResultRow] {
        struct Params: Encodable { let queryId: UInt64; let offset: Int; let limit: Int }
        return try await detached { try self.value("rows", Params(queryId: queryID, offset: offset, limit: limit), as: [ResultRow].self) }
    }
    /// IDs of the selectable rows in `offset..<offset + limit` of a query, without row data.
    public func eligibleIDs(queryID: UInt64, offset: Int, limit: Int) async throws -> [String] {
        struct Params: Encodable { let queryId: UInt64; let offset: Int; let limit: Int }
        return try await detached { try self.value("eligibleIds", Params(queryId: queryID, offset: offset, limit: limit), as: [String].self) }
    }
    public func position(queryID: UInt64, id: String) async -> Int? {
        struct Params: Encodable { let queryId: UInt64; let id: String }
        return (try? await detached { try self.call("position", Params(queryId: queryID, id: id), as: Int.self) }) ?? nil
    }
    public func finding(_ id: String) async -> Finding? {
        (try? await detached { try self.call("finding", ["id": id], as: Finding.self) }) ?? nil
    }
    public func selection(_ ids: [String], preview: Int = 200) async -> SelectionSummary {
        struct Params: Encodable { let ids: [String]; let preview: Int }
        return (try? await detached { try self.value("selection", Params(ids: ids, preview: preview), as: SelectionSummary.self) }) ?? .empty
    }
    public func executeSelection(_ ids: [String], kind: ActionKind, context: ScanContext) async throws -> [ActionResult] {
        struct Params: Encodable { let ids: [String]; let kind: ActionKind; let context: ScanContext }
        return try await detached { try self.value("executeSelection", Params(ids: ids, kind: kind, context: context), as: [ActionResult].self) }
    }
    public func removeResults(_ ids: [String]) async {
        _ = try? await detached { try self.call("removeResults", ["ids": ids], as: Int.self) }
    }
    public func loadResults(_ findings: [Finding]) async {
        _ = try? await detached { try self.call("loadResults", ["findings": findings], as: Int.self) }
    }
    /// Replaces stored results with synthetic rows, for previews and scale checks.
    public func synthesize(_ count: Int) async -> Int {
        (try? await detached { try self.value("synthesize", ["count": count], as: Int.self) }) ?? 0
    }

    /// Streams a scan. Cancelling the consuming task cancels the core scan; a core-side
    /// cancellation finishes the stream with `CancellationError`. With `store`, findings go to
    /// the core's result store and only `.stored` counts are streamed.
    public func scan(moduleIDs: [String], context: ScanContext, concurrency: Int = 3, store: Bool = false) -> AsyncThrowingStream<ScanEvent, Error> {
        struct ScanRequest: Encodable { let modules: [String]; let context: ScanContext; let concurrency: Int; let store: Bool }
        return AsyncThrowingStream { continuation in
            guard let data = try? Self.encoder.encode(ScanRequest(modules: moduleIDs, context: context, concurrency: concurrency, store: store)) else {
                continuation.finish(throwing: CleanError.message("The scan request could not be encoded.")); return
            }
            let box = Unmanaged.passRetained(ScanBox(continuation))
            let started = String(decoding: data, as: UTF8.self).withCString { request in
                cym_scan_start(handle, request, { context, json in
                    guard let context, let json else { return }
                    let done = Unmanaged<ScanBox>.fromOpaque(context).takeUnretainedValue().receive(json)
                    // The finished event is the last use of the context.
                    if done { Unmanaged<ScanBox>.fromOpaque(context).release() }
                }, box.toOpaque())
            }
            guard let started else {
                box.release()
                continuation.finish(throwing: CleanError.message("The core rejected the scan request.")); return
            }
            let scan = ScanHandle(started)
            continuation.onTermination = { _ in scan.cancel() }
        }
    }
}

private final class ScanHandle: @unchecked Sendable {
    private let handle: OpaquePointer
    init(_ handle: OpaquePointer) { self.handle = handle }
    func cancel() { cym_scan_cancel(handle) }
    deinit { cym_scan_free(handle) }
}

private final class ScanBox: @unchecked Sendable {
    private struct Event: Decodable {
        let type: String
        let moduleId: String?
        let finding: Finding?
        let message: String?
        let cancelled: Bool?
        let total: Int?
    }
    private let continuation: AsyncThrowingStream<ScanEvent, Error>.Continuation
    init(_ continuation: AsyncThrowingStream<ScanEvent, Error>.Continuation) { self.continuation = continuation }

    /// Returns true for the final event.
    func receive(_ json: UnsafePointer<CChar>) -> Bool {
        guard let event = try? CoreEngine.decoder.decode(Event.self, from: Data(String(cString: json).utf8)) else {
            continuation.yield(.warning("A scan event could not be decoded.")); return false
        }
        switch event.type {
        case "finding": if let finding = event.finding { continuation.yield(.finding(finding)) }
        case "progress": continuation.yield(.progress(event.message ?? ""))
        case "warning": continuation.yield(.warning(event.message ?? ""))
        case "moduleFinished": continuation.yield(.moduleFinished(event.moduleId ?? ""))
        case "stored": continuation.yield(.stored(moduleID: event.moduleId ?? "", total: event.total ?? 0))
        case "finished":
            if event.cancelled == true { continuation.finish(throwing: CancellationError()) } else { continuation.finish() }
            return true
        default: break
        }
        return false
    }
}
