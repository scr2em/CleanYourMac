import CleanYourMacCore
import CryptoKit
import Darwin
import Foundation

public struct FileEntry: Sendable {
    public let path: String
    public let isDirectory: Bool
    public let isRegular: Bool
    public let isSymbolicLink: Bool
    public let bytes: UInt64
    public let allocated: UInt64
    public let modified: Date
    public let identity: FileIdentity
}

public struct FileSize: Sendable {
    public var logical: UInt64 = 0
    public var allocated: UInt64 = 0
    public var files: Int = 0
    public var complete = true
    public var signature: String?
    public init(logical: UInt64 = 0, allocated: UInt64 = 0, files: Int = 0, complete: Bool = true, signature: String? = nil) {
        self.logical = logical; self.allocated = allocated; self.files = files; self.complete = complete
        self.signature = signature
    }
}

public protocol TrashMoving: Sendable {
    func move(_ path: String) throws -> String
}
public struct NativeTrashMover: TrashMoving {
    public init() {}
    public func move(_ path: String) throws -> String {
        var result: NSURL?
        try FileManager.default.trashItem(at: URL(fileURLWithPath: path), resultingItemURL: &result)
        guard let path = result?.path else { throw CleanError.message("Trash operation returned no local destination.") }
        return path
    }
}
public struct FileService: Sendable {
    private let trash: any TrashMoving
    public init(trash: any TrashMoving = NativeTrashMover()) { self.trash = trash }
    public func hash(_ path: String) throws -> String {
        let before = try entry(path)
        guard before.isRegular else { throw CleanError.message("Only regular files can be hashed.") }
        let handle = try FileHandle(forReadingFrom: URL(fileURLWithPath: path))
        defer { try? handle.close() }
        var hash = SHA256()
        while let data = try handle.read(upToCount: 1_048_576), !data.isEmpty {
            try Task.checkCancellation()
            hash.update(data: data)
        }
        guard try entry(path).identity == before.identity else { throw CleanError.message("File changed during duplicate verification.") }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }

    public func entry(_ path: String) throws -> FileEntry {
        try inspect(path, allowLink: false)
    }

    private func inspect(_ path: String, allowLink: Bool) throws -> FileEntry {
        let path = PathPolicy.canonical(path)
        var info = stat()
        guard lstat(path, &info) == 0 else { throw CleanError.message("Cannot inspect \(path): \(String(cString: strerror(errno)))") }
        let kind = info.st_mode & S_IFMT
        guard allowLink || kind != S_IFLNK else { throw CleanError.message("Symbolic links are not traversed: \(path)") }
        let identity = FileIdentity(path: path, device: UInt64(info.st_dev), inode: UInt64(info.st_ino), modifiedSeconds: Int64(info.st_mtimespec.tv_sec), modifiedNanos: Int64(info.st_mtimespec.tv_nsec))
        return FileEntry(path: path, isDirectory: kind == S_IFDIR, isRegular: kind == S_IFREG, isSymbolicLink: kind == S_IFLNK, bytes: UInt64(max(info.st_size, 0)), allocated: UInt64(max(info.st_blocks, 0)) * 512, modified: Date(timeIntervalSince1970: Double(info.st_mtimespec.tv_sec)), identity: identity)
    }

    public func noSymlinkAncestors(_ path: String) throws {
        let components = PathPolicy.canonical(path).split(separator: "/")
        var ancestor = ""
        for component in components {
            ancestor += "/" + component
            _ = try entry(ancestor)
        }
    }

    public func roots(_ paths: [String]) -> [String] {
        var canonical: [String] = []
        var physicalRoots = Set<String>()
        for path in Set(paths.map { URL(fileURLWithPath: PathPolicy.canonical($0)).resolvingSymlinksInPath().path }).sorted() {
            if let item = try? entry(path), !physicalRoots.insert("\(item.identity.device):\(item.identity.inode)").inserted { continue }
            canonical.append(path)
        }
        return canonical.filter { path in !canonical.contains { $0 != path && PathPolicy.contains(path, in: $0) } }.sorted()
    }

    public func walk(
        context: ScanContext,
        limit: Int = 300_000,
        visit: (FileEntry) throws -> Bool,
        warning: @escaping @Sendable (String) -> Void
    ) throws {
        var count = 0
        for root in roots(context.roots) {
            try Task.checkCancellation()
            guard !context.excludes(root), !PathPolicy.traversalExcluded(root) else { continue }
            guard let rootEntry = try? entry(root), rootEntry.isDirectory else { warning("Root is unavailable: \(root)"); continue }
            guard let enumerator = FileManager.default.enumerator(at: URL(fileURLWithPath: root), includingPropertiesForKeys: nil, options: [.skipsPackageDescendants], errorHandler: { url, error in
                warning("Skipped \(url.path): \(error.localizedDescription)")
                return true
            }) else { warning("Cannot enumerate \(root)"); continue }
            for case let url as URL in enumerator {
                try Task.checkCancellation()
                count += 1
                if count > limit { warning("Scan reached its \(limit)-entry limit; narrow the roots for complete coverage."); return }
                if context.excludes(url.path) || PathPolicy.traversalExcluded(url.path) {
                    enumerator.skipDescendants(); continue
                }
                guard let item = try? entry(url.path) else { enumerator.skipDescendants(); continue }
                if try !visit(item) { enumerator.skipDescendants() }
            }
        }
    }

    public func size(_ path: String, limit: Int = 500_000) throws -> FileSize {
        let root = try entry(path)
        if !root.isDirectory { return FileSize(logical: root.bytes, allocated: root.allocated, files: 1, complete: true) }
        var result = FileSize()
        var identities = Set<String>()
        let coverage = EnumerationCoverage()
        var digest = [UInt8](repeating: 0, count: 32)
        var entries = 0
        guard let enumerator = FileManager.default.enumerator(at: URL(fileURLWithPath: path), includingPropertiesForKeys: nil, options: [], errorHandler: { _, _ in coverage.fail(); return true }) else { return FileSize(complete: false) }
        for case let url as URL in enumerator {
            try Task.checkCancellation()
            if entries >= limit { result.complete = false; break }
            guard let file = try? inspect(url.path, allowLink: true) else { result.complete = false; enumerator.skipDescendants(); continue }
            if file.isSymbolicLink { enumerator.skipDescendants() }
            entries += 1
            let relative = String(file.path.dropFirst(root.path.count))
            let metadata = [relative, String(file.identity.device), String(file.identity.inode), String(file.identity.modifiedSeconds), String(file.identity.modifiedNanos), String(file.bytes), String(file.isDirectory), String(file.isSymbolicLink)].joined(separator: "\0")
            for (index, byte) in SHA256.hash(data: Data(metadata.utf8)).enumerated() { digest[index] ^= byte }
            if file.isRegular || file.isSymbolicLink {
                result.files += 1
                result.logical += file.bytes
                if identities.insert("\(file.identity.device):\(file.identity.inode)").inserted { result.allocated += file.allocated }
            }
        }
        result.complete = result.complete && coverage.complete
        result.signature = "\(entries):" + digest.map { String(format: "%02x", $0) }.joined()
        return result
    }
    public func snapshotIdentity(_ path: String) throws -> FileIdentity {
        let item = try entry(path)
        return item.isDirectory ? item.identity.withTreeSignature(try size(path).signature) : item.identity
    }
    public func validateIdentity(_ expected: FileIdentity) throws {
        let current = try entry(expected.path).identity
        guard current == expected.withTreeSignature(nil) else { throw CleanError.message("The item changed after scanning. Scan again before acting.") }
        if let signature = expected.treeSignature {
            let size = try size(expected.path)
            guard size.complete, size.signature == signature else { throw CleanError.message("Folder contents changed after scanning or cannot be verified. Scan again.") }
        }
    }

    public func validate(_ expected: FileIdentity, context: ScanContext, additionalRoots: [String] = []) throws {
        guard !PathPolicy.protected(expected.path), !context.protectsSelection(expected.path) else { throw CleanError.message("This path or an item inside it is protected or excluded.") }
        guard (context.roots + additionalRoots).contains(where: { PathPolicy.contains(expected.path, in: $0) }) else { throw CleanError.message("This item is outside the approved scan scope.") }
        try noSymlinkAncestors(expected.path)
        try validateIdentity(expected)
    }

    public func moveToTrash(_ identity: FileIdentity, context: ScanContext, additionalRoots: [String] = []) throws -> String {
        try validate(identity, context: context, additionalRoots: additionalRoots)
        return try trash.move(identity.path)
    }

    public func restore(_ result: ActionResult) throws {
        guard let original = result.originalPath, let trashed = result.trashPath else { throw CleanError.message("This action has no restorable item.") }
        guard !FileManager.default.fileExists(atPath: original) else { throw CleanError.message("An item already exists at the original path.") }
        guard FileManager.default.fileExists(atPath: trashed) else { throw CleanError.message("The item no longer exists in Trash.") }
        guard let expected = result.trashIdentity else { throw CleanError.message("Restore identity is unavailable. Inspect Trash in Finder.") }
        try validateIdentity(expected)
        try noSymlinkAncestors(trashed)
        let parent = (original as NSString).deletingLastPathComponent
        try noSymlinkAncestors(parent)
        try FileManager.default.moveItem(atPath: trashed, toPath: original)
    }
}

private final class EnumerationCoverage: @unchecked Sendable {
    private let lock = NSLock()
    private var successful = true
    func fail() { lock.lock(); successful = false; lock.unlock() }
    var complete: Bool { lock.lock(); defer { lock.unlock() }; return successful }
}

public enum KnownLocations {
    public static var home: String { NSHomeDirectory() }
    public static var projectRoots: [String] {
        ["projects", "Projects", "Code", "dev", "GitHub", "Workspace"].map { home + "/" + $0 }.filter { FileManager.default.fileExists(atPath: $0) }
    }
    public static var xcode: [String] {
        ["DerivedData", "iOS DeviceSupport", "Archives"].map { home + "/Library/Developer/Xcode/" + $0 }
    }
    public static var caches: [(String, String)] {
        [
            ("npm", home + "/.npm/_cacache"), ("Yarn", home + "/Library/Caches/Yarn"),
            ("Bun", home + "/.bun/install/cache"), ("Homebrew", home + "/Library/Caches/Homebrew"),
            ("pip", home + "/Library/Caches/pip"), ("uv", home + "/.cache/uv"),
            ("Poetry", home + "/Library/Caches/pypoetry"), ("SwiftPM", home + "/Library/Caches/org.swift.swiftpm"),
            ("CocoaPods", home + "/Library/Caches/CocoaPods"), ("Cargo cache", home + "/.cargo/registry/cache"),
            ("Go build cache", home + "/Library/Caches/go-build"), ("Gradle caches", home + "/.gradle/caches"),
        ]
    }
    public static func roots(for module: String) -> [String] {
        switch module {
        case "xcode": xcode
        case "caches": caches.map(\.1) + [home + "/Library/Caches", home + "/Library/Logs"]
        case "applications", "leftovers": ["/Applications", home + "/Applications", home + "/Library/Caches", home + "/Library/Preferences", home + "/Library/Saved Application State", home + "/Library/Application Support"]
        case "downloads": [home + "/Downloads"]
        case "trash": [home + "/.Trash"]
        default: []
        }
    }
}
