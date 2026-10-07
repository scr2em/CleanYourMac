import Foundation

public enum Category: String, Codable, CaseIterable, Sendable {
    case storage = "Storage", developer = "Developer", applications = "Applications", tools = "Tools"
}

public struct ModuleDescriptor: Identifiable, Hashable, Sendable {
    public let id: String
    public let name: String
    public let category: Category
    public let symbol: String
    public let summary: String
    public let usesRoots: Bool
    public init(id: String, name: String, category: Category, symbol: String, summary: String, usesRoots: Bool = true) {
        self.id = id; self.name = name; self.category = category; self.symbol = symbol; self.summary = summary; self.usesRoots = usesRoots
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
    public func withTreeSignature(_ signature: String?) -> FileIdentity {
        FileIdentity(path: path, device: device, inode: inode, modifiedSeconds: modifiedSeconds, modifiedNanos: modifiedNanos, treeSignature: signature)
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

public enum Resource: Codable, Hashable, Sendable {
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

public enum Risk: String, Codable, Sendable { case review = "Review", rebuild = "Rebuild required", permanent = "Permanent" }

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
        case .removeWorktree: "Git removes the selected linked worktree. Ignored files may be lost. The branch is retained. This action cannot be undone here."
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
    public var details: [Detail]
    public var actions: [ActionKind]
    public var risk: Risk
    public var reason: String
    public var blockedReason: String?
    public var badge: String?
    public init(id: String, moduleID: String, title: String, subtitle: String, resource: Resource, bytes: UInt64? = nil, allocatedBytes: UInt64? = nil, cpuPercent: Double? = nil, memoryBytes: UInt64? = nil, modifiedAt: Date? = nil, details: [Detail] = [], actions: [ActionKind] = [], risk: Risk = .review, reason: String, blockedReason: String? = nil, badge: String? = nil) {
        self.id = id; self.moduleID = moduleID; self.title = title; self.subtitle = subtitle; self.resource = resource
        self.bytes = bytes; self.allocatedBytes = allocatedBytes; self.cpuPercent = cpuPercent; self.memoryBytes = memoryBytes
        self.modifiedAt = modifiedAt
        self.details = details; self.actions = actions; self.risk = risk; self.reason = reason; self.blockedReason = blockedReason; self.badge = badge
    }
}

public struct ScanContext: Sendable {
    public let roots: [String]
    public let exclusions: [String]
    public let ignoredProcessNames: Set<String>
    public let limitToRoots: Bool
    public init(roots: [String], exclusions: [String] = [], ignoredProcessNames: Set<String> = ["ssh-agent", "gpg-agent", "keyboxd", "dirmngr"], limitToRoots: Bool = false) {
        self.roots = roots; self.exclusions = exclusions; self.ignoredProcessNames = ignoredProcessNames; self.limitToRoots = limitToRoots
    }
    public func excludes(_ path: String) -> Bool { exclusions.contains { PathPolicy.contains(path, in: $0) } }
    public func allows(_ path: String) -> Bool {
        !excludes(path) && (!limitToRoots || roots.contains { PathPolicy.contains(path, in: $0) })
    }
    public func protectsSelection(_ path: String) -> Bool {
        excludes(path) || exclusions.contains { PathPolicy.contains($0, in: path) }
    }
}

public enum ScanEvent: Sendable {
    case finding(Finding)
    case progress(String)
    case warning(String)
}

public protocol ScanModule: Sendable {
    var descriptor: ModuleDescriptor { get }
    func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error>
}

public struct ModuleRegistry: Sendable {
    public let modules: [any ScanModule]
    public init(_ modules: [any ScanModule]) {
        precondition(Set(modules.map(\.descriptor.id)).count == modules.count, "Module IDs must be unique")
        self.modules = modules
    }
    public func module(_ id: String) -> (any ScanModule)? { modules.first { $0.descriptor.id == id } }
}

public struct ActionRequest: Sendable {
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
    public init(title: String, originalPath: String?, action: ActionKind, outcome: Outcome, message: String, trashPath: String? = nil, findingID: String? = nil, trashIdentity: FileIdentity? = nil) {
        id = UUID(); date = Date(); self.title = title; self.originalPath = originalPath
        self.action = action; self.outcome = outcome; self.message = message; self.trashPath = trashPath
        self.findingID = findingID; self.trashIdentity = trashIdentity
    }
}

public enum CleanError: LocalizedError, Sendable {
    case message(String)
    public var errorDescription: String? { if case .message(let message) = self { message } else { nil } }
}

public enum PathPolicy {
    public static func canonical(_ path: String) -> String {
        URL(fileURLWithPath: (path as NSString).expandingTildeInPath).standardizedFileURL.path
    }
    public static func contains(_ path: String, in root: String) -> Bool {
        let path = canonical(path), root = canonical(root)
        return path == root || path.hasPrefix(root == "/" ? "/" : root + "/")
    }
    public static func protected(_ path: String, home: String = NSHomeDirectory()) -> Bool {
        let path = canonical(path)
        let exact = ["/", home, home + "/Library", home + "/Documents", home + "/Desktop", home + "/Downloads", "/Applications", home + "/Applications"]
        let prefixes = ["/System", "/bin", "/sbin", "/usr", "/private", home + "/.ssh", home + "/.aws", home + "/.gnupg", home + "/Library/Keychains", home + "/Library/Mobile Documents", home + "/Library/CloudStorage"]
        return exact.contains(path) || prefixes.contains { contains(path, in: $0) } || path.split(separator: "/").contains { $0 == ".git" || $0 == ".env" || $0.hasPrefix(".env.") }
    }
    public static func traversalExcluded(_ path: String) -> Bool {
        let home = NSHomeDirectory()
        let prefixes = ["/System", "/Library", "/bin", "/sbin", "/usr", "/dev", "/Network", "/private", "/var", "/etc", "/tmp", home + "/.ssh", home + "/.aws", home + "/.gnupg", home + "/Library/Keychains", home + "/Library/Mobile Documents", home + "/Library/CloudStorage"]
        return prefixes.contains { contains(path, in: $0) } || path.split(separator: "/").contains { $0 == ".git" || $0 == ".env" || $0.hasPrefix(".env.") }
    }
    /// Duplicate inspection skips generated outputs and dependency stores in every ecosystem.
    public static let duplicateIgnoredDirectories: Set<String> = [
        "node_modules", "packages", ".git", "build", ".build", "dist", "out", "target", "bin", "obj",
        ".next", ".nuxt", ".turbo", ".parcel-cache", "coverage", ".cache", ".gradle", ".m2",
        "vendor", "pods", "carthage", ".swiftpm", "deriveddata", ".venv", "venv", "env",
        "__pycache__", ".tox", ".nox", ".pytest_cache", ".mypy_cache", ".ruff_cache", "site-packages",
        ".dart_tool", ".pub-cache", ".pub", ".bundle", ".cargo", ".cabal", "_build", "deps", "bower_components"
    ]
    public static func duplicateExcluded(_ path: String) -> Bool {
        traversalExcluded(path) || path.split(separator: "/").contains { duplicateIgnoredDirectories.contains($0.lowercased()) }
    }
    public static func normalizedSelection(_ findings: [Finding]) -> [Finding] {
        let unique = Dictionary(findings.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }).values
        return unique.filter { finding in
            guard let path = finding.resource.path, case .file = finding.resource else { return true }
            return !unique.contains { other in
                guard other.id != finding.id, case .file = other.resource, let parent = other.resource.path else { return false }
                return parent != path && contains(path, in: parent)
            }
        }.sorted { $0.id < $1.id }
    }
}
