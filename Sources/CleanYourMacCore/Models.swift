import Foundation

// Wire models decoded from the Rust core. Dates cross the boundary as Unix seconds.

public enum Category: String, Codable, CaseIterable, Sendable {
    case storage = "Storage", developer = "Developer", applications = "Applications", tools = "Tools"
}

public struct ModuleDescriptor: Identifiable, Hashable, Codable, Sendable {
    public let id: String
    public let name: String
    public let category: Category
    public let symbol: String
    public let summary: String
    public let usesRoots: Bool
    /// Whether the overview's scan runs this module; otherwise only its own page scans it.
    public let inOverview: Bool
    public init(id: String, name: String, category: Category, symbol: String, summary: String, usesRoots: Bool = true, inOverview: Bool = true) {
        self.id = id; self.name = name; self.category = category; self.symbol = symbol; self.summary = summary; self.usesRoots = usesRoots; self.inOverview = inOverview
    }
}

public struct FileIdentity: Codable, Hashable, Sendable {
    public let path: String
    public let device: UInt64
    public let inode: UInt64
    public let modifiedSeconds: Int64
    public let modifiedNanos: Int64
    public let treeSignature: String?
    public init(path: String, device: UInt64, inode: UInt64, modifiedSeconds: Int64, modifiedNanos: Int64, treeSignature: String? = nil) {
        self.path = path; self.device = device; self.inode = inode; self.modifiedSeconds = modifiedSeconds; self.modifiedNanos = modifiedNanos
        self.treeSignature = treeSignature
    }
}

public struct ProcessIdentity: Codable, Hashable, Sendable {
    public let pid: Int32
    public let uid: UInt32
    public let startedSeconds: UInt64
    public let startedMicroseconds: UInt64
    public let executable: String
    public init(pid: Int32, uid: UInt32, startedSeconds: UInt64, startedMicroseconds: UInt64, executable: String) {
        self.pid = pid; self.uid = uid; self.startedSeconds = startedSeconds; self.startedMicroseconds = startedMicroseconds; self.executable = executable
    }
    public var key: String { "\(pid):\(startedSeconds):\(startedMicroseconds)" }
}

public enum Resource: Hashable, Sendable {
    case file(FileIdentity)
    case process(ProcessIdentity)
    case worktree(file: FileIdentity, repository: String, head: String)
    case worktreeRegistration(path: String, repository: String)
    case simulator(id: String, state: String)

    public var path: String? {
        switch self {
        case .file(let file), .worktree(let file, _, _): file.path
        case .worktreeRegistration(let path, _): path
        case .process(let process): process.executable
        case .simulator: nil
        }
    }
}

// Matches the core's internally tagged representation: {"kind": "file", "file": {...}}.
extension Resource: Codable {
    private enum Keys: String, CodingKey { case kind, file, process, repository, head, path, id, state }
    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "file": self = .file(try c.decode(FileIdentity.self, forKey: .file))
        case "process": self = .process(try c.decode(ProcessIdentity.self, forKey: .process))
        case "worktree": self = .worktree(file: try c.decode(FileIdentity.self, forKey: .file), repository: try c.decode(String.self, forKey: .repository), head: try c.decode(String.self, forKey: .head))
        case "worktreeRegistration": self = .worktreeRegistration(path: try c.decode(String.self, forKey: .path), repository: try c.decode(String.self, forKey: .repository))
        case "simulator": self = .simulator(id: try c.decode(String.self, forKey: .id), state: try c.decode(String.self, forKey: .state))
        case let kind: throw DecodingError.dataCorruptedError(forKey: .kind, in: c, debugDescription: "Unknown resource kind \(kind)")
        }
    }
    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .file(let file): try c.encode("file", forKey: .kind); try c.encode(file, forKey: .file)
        case .process(let process): try c.encode("process", forKey: .kind); try c.encode(process, forKey: .process)
        case .worktree(let file, let repository, let head):
            try c.encode("worktree", forKey: .kind); try c.encode(file, forKey: .file)
            try c.encode(repository, forKey: .repository); try c.encode(head, forKey: .head)
        case .worktreeRegistration(let path, let repository):
            try c.encode("worktreeRegistration", forKey: .kind); try c.encode(path, forKey: .path); try c.encode(repository, forKey: .repository)
        case .simulator(let id, let state):
            try c.encode("simulator", forKey: .kind); try c.encode(id, forKey: .id); try c.encode(state, forKey: .state)
        }
    }
}

public enum Risk: String, Codable, Sendable { case review = "Review", rebuild = "Rebuild required", permanent = "Permanent" }

/// A one-click fix the core recommends from the current results.
public struct Recommendation: Codable, Identifiable, Hashable, Sendable {
    public let id: String
    public let moduleID: String
    public let title: String
    public let detail: String
    public let action: ActionKind
    public let risk: Risk
    /// Rows the fix applies to.
    public let count: Int
    /// Bytes freed, with nested folders counted once.
    public let bytes: UInt64
    /// The most common ecosystem logo among the rows.
    public let brand: String?
    public let ids: [String]
    private enum CodingKeys: String, CodingKey {
        case id, moduleID = "moduleId", title, detail, action, risk, count, bytes, brand, ids
    }
}

public enum ActionKind: String, Codable, CaseIterable, Sendable {
    case trash, removeWorktree, resetSimulator, deleteSimulator, terminate, forceQuit, emptyTrash
    public var label: String {
        switch self {
        case .trash: "Move to Trash"
        case .removeWorktree: "Remove Worktree"
        case .resetSimulator: "Reset Simulator"
        case .deleteSimulator: "Delete Simulator"
        case .terminate: "Terminate"
        case .forceQuit: "Force Quit"
        case .emptyTrash: "Empty Trash"
        }
    }
    public var consequence: String {
        switch self {
        case .trash: "Moves the selected items to Trash. Disk space is freed only after Trash is emptied. Restore is available while the item still exists."
        case .removeWorktree: "Git deletes the worktree folder, including the ignored files listed for it. Its branch and commits stay in the repository. This action cannot be undone here."
        case .resetSimulator: "Erases apps, data and settings on the selected simulator. The device remains. This cannot be undone."
        case .deleteSimulator: "Deletes the selected simulator and all its app data. Its runtime remains installed. This cannot be undone."
        case .terminate: "Requests a graceful exit with SIGTERM. A process may have unsaved work. No automatic force quit follows."
        case .forceQuit: "Sends SIGKILL to the selected process. Unsaved work may be lost. This cannot be undone."
        case .emptyTrash: "Permanently removes the specifically reviewed Trash items. This cannot be undone."
        }
    }
}

public struct Detail: Codable, Hashable, Sendable {
    public let label: String
    public let value: String
    public init(_ label: String, _ value: String) { self.label = label; self.value = value }
}

public struct Finding: Identifiable, Codable, Hashable, Sendable {
    public let id: String
    public let moduleID: String
    public let title: String
    public let subtitle: String
    public let resource: Resource
    public var bytes: UInt64?
    public var allocatedBytes: UInt64?
    public var cpuPercent: Double?
    public var memoryBytes: UInt64?
    public var modifiedAt: Date?
    /// When the item (or, for generated folders, its project) was last used.
    public var lastUsedAt: Date?
    public var details: [Detail]
    public var actions: [ActionKind]
    public var risk: Risk
    public var reason: String
    public var blockedReason: String?
    public var badge: String?
    public var brand: String?
    private enum CodingKeys: String, CodingKey {
        case id, moduleID = "moduleId", title, subtitle, resource, bytes, allocatedBytes, cpuPercent, memoryBytes, modifiedAt, lastUsedAt, details, actions, risk, reason, blockedReason, badge, brand
    }
    public init(id: String, moduleID: String, title: String, subtitle: String, resource: Resource, bytes: UInt64? = nil, allocatedBytes: UInt64? = nil, cpuPercent: Double? = nil, memoryBytes: UInt64? = nil, modifiedAt: Date? = nil, lastUsedAt: Date? = nil, details: [Detail] = [], actions: [ActionKind] = [], risk: Risk = .review, reason: String, blockedReason: String? = nil, badge: String? = nil, brand: String? = nil) {
        self.id = id; self.moduleID = moduleID; self.title = title; self.subtitle = subtitle; self.resource = resource
        self.bytes = bytes; self.allocatedBytes = allocatedBytes; self.cpuPercent = cpuPercent; self.memoryBytes = memoryBytes
        self.modifiedAt = modifiedAt; self.lastUsedAt = lastUsedAt
        self.details = details; self.actions = actions; self.risk = risk; self.reason = reason; self.blockedReason = blockedReason; self.badge = badge; self.brand = brand
    }
    public func value(_ label: String) -> String? { details.first { $0.label == label }?.value }
}

public struct ScanContext: Codable, Sendable {
    public let roots: [String]
    public let exclusions: [String]
    public let ignoredProcessNames: [String]
    public let limitToRoots: Bool
    public init(roots: [String], exclusions: [String] = [], ignoredProcessNames: [String] = ["ssh-agent", "gpg-agent", "keyboxd", "dirmngr"], limitToRoots: Bool = false) {
        self.roots = roots; self.exclusions = exclusions; self.ignoredProcessNames = ignoredProcessNames; self.limitToRoots = limitToRoots
    }
}

public struct ActionRequest: Codable, Sendable {
    public let findings: [Finding]
    public let kind: ActionKind
    public let context: ScanContext
    public init(findings: [Finding], kind: ActionKind, context: ScanContext) {
        self.findings = findings; self.kind = kind; self.context = context
    }
}

public enum Outcome: String, Codable, Sendable { case applied, skipped, failed, requested }

public struct ActionResult: Identifiable, Codable, Sendable {
    public let id: UUID
    public let date: Date
    public let title: String
    public let originalPath: String?
    public let action: ActionKind
    public let outcome: Outcome
    public let message: String
    public let trashPath: String?
    public let findingID: String?
    public let trashIdentity: FileIdentity?
    public var journalWarning: String?
    private enum CodingKeys: String, CodingKey {
        case id, date, title, originalPath, action, outcome, message, trashPath, findingID = "findingId", trashIdentity, journalWarning
    }
}

public struct ModuleTotal: Codable, Hashable, Sendable {
    public let moduleId: String
    public let count: Int
    public let bytes: UInt64
    public let reclaimableBytes: UInt64
}

/// Aggregate figures computed by the core. Disk totals count overlapping paths once; process
/// memory is reported separately.
public struct Analytics: Codable, Hashable, Sendable {
    public let findings: Int
    public let diskBytes: UInt64
    public let allocatedBytes: UInt64
    public let reclaimableBytes: UInt64
    public let blocked: Int
    public let processCount: Int
    public let processMemoryBytes: UInt64
    public let modules: [ModuleTotal]
    public static let empty = Analytics(findings: 0, diskBytes: 0, allocatedBytes: 0, reclaimableBytes: 0, blocked: 0, processCount: 0, processMemoryBytes: 0, modules: [])
}

public enum ScanEvent: Sendable {
    case finding(Finding)
    case progress(String)
    case warning(String)
    case moduleFinished(String)
    /// Findings were written to the core's result store; `total` counts every stored row.
    case stored(moduleID: String, total: Int)
}

/// A lightweight list row projected from a stored finding.
public struct ResultRow: Identifiable, Codable, Hashable, Sendable {
    public let id: String
    public let moduleID: String
    public let title: String
    public let subtitle: String
    public let path: String?
    public let pid: Int32?
    public let bytes: UInt64?
    public let allocatedBytes: UInt64?
    public let cpuPercent: Double?
    public let memoryBytes: UInt64?
    public let modifiedAt: Date?
    public let lastUsedAt: Date?
    public let risk: Risk
    public let badge: String?
    public let blocked: Bool
    public let eligible: Bool
    /// The ecosystem's logo slug, when the core knows one.
    public let brand: String?
    public var isProcess: Bool { pid != nil }
    private enum CodingKeys: String, CodingKey {
        case id, moduleID = "moduleId", title, subtitle, path, pid, bytes, allocatedBytes, cpuPercent, memoryBytes, modifiedAt, lastUsedAt, risk, badge, blocked, eligible, brand
    }
}

public enum ResultSort: String, Codable, Sendable { case size, name, cpu, lastUsed }

/// What a finder is looking at. The core filters and sorts; the app pages through the result.
public struct ResultQuery: Codable, Hashable, Sendable {
    public var module: String?
    public var search: String
    public var minBytes: UInt64
    public var modifiedBefore: Date?
    public var sort: ResultSort
    public var ascending: Bool
    public init(module: String?, search: String = "", minBytes: UInt64 = 0, modifiedBefore: Date? = nil, sort: ResultSort = .size, ascending: Bool = false) {
        self.module = module; self.search = search; self.minBytes = minBytes; self.modifiedBefore = modifiedBefore; self.sort = sort; self.ascending = ascending
    }
}

public struct QueryInfo: Codable, Sendable {
    public let queryId: UInt64
    public let generation: UInt64
    public let total: Int
    public let summary: Analytics
    public let largest: [ResultRow]
    public let maxCpu: Double?
    /// Rows an action can apply to.
    public let eligible: Int
}

/// Totals and shared actions for selected IDs, with a bounded preview for review.
public struct SelectionSummary: Codable, Sendable {
    public let count: Int
    public let bytes: UInt64
    public let actions: [ActionKind]
    public let includesProcesses: Bool
    public let preview: [Finding]
    public static let empty = SelectionSummary(count: 0, bytes: 0, actions: [], includesProcesses: false, preview: [])
}

public enum CleanError: LocalizedError, Sendable {
    case message(String)
    public var errorDescription: String? { if case .message(let message) = self { message } else { nil } }
}
