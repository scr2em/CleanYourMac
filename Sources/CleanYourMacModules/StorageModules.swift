import AppKit
import CleanYourMacCore
import CleanYourMacPlatform
import Foundation

public struct StorageModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "storage", name: "Storage Explorer", category: .storage, symbol: "internaldrive", summary: "Size the immediate contents of chosen folders. Reveal folders to inspect them.")
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            for root in FileService().roots(context.roots) {
                for entry in Streams.children(root, continuation: stream) where context.allows(entry.path) && !PathPolicy.traversalExcluded(entry.path) {
                    try Task.checkCancellation()
                    stream.yield(.progress("Sizing \((entry.path as NSString).lastPathComponent)"))
                    stream.yield(.finding(try Streams.file(entry, module: "storage", reason: "Storage inventory, not a cleanup recommendation. Inspect this item in Finder.", actions: [])))
                }
            }
        }
    }
}

public struct LargeFilesModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "large", name: "Large Files", category: .storage, symbol: "doc", summary: "Files at least 100 MB, with dates and explicit review before removal.")
    public let minimumBytes: UInt64
    public init(minimumBytes: UInt64 = 100_000_000) { self.minimumBytes = minimumBytes }
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            try FileService().walk(context: context, visit: { entry in
                if entry.isRegular && entry.bytes >= minimumBytes {
                    stream.yield(.finding(Finding(id: "large:" + entry.path, moduleID: "large", title: (entry.path as NSString).lastPathComponent, subtitle: entry.path, resource: .file(entry.identity), bytes: entry.bytes, allocatedBytes: entry.allocated, modifiedAt: entry.modified, details: [Detail("Modified", entry.modified.formatted())], actions: PathPolicy.protected(entry.path) ? [] : [.trash], reason: "Large personal file. Size or age alone does not make this disposable; inspect its contents before removal.")))
                }
                return true
            }, warning: { stream.yield(.warning($0)) })
        }
    }
}

public struct DuplicateModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "duplicates", name: "Exact Duplicates", category: .storage, symbol: "doc.on.doc", summary: "Verify equal contents with SHA-256; preserve one original in each group.")
    public let minimumBytes: UInt64
    public init(minimumBytes: UInt64 = 4_096) { self.minimumBytes = minimumBytes }
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            let files = FileService()
            var sizes: [UInt64: [FileEntry]] = [:]
            try files.walk(context: context, visit: { entry in
                if entry.isRegular && entry.bytes >= minimumBytes { sizes[entry.bytes, default: []].append(entry) }
                return !entry.isDirectory || !PathPolicy.duplicateExcluded(entry.path)
            }, warning: { stream.yield(.warning($0)) })
            for entries in sizes.values where entries.count > 1 {
                var hashes: [String: [FileEntry]] = [:]
                var uniqueInodes = Set<String>()
                for entry in entries {
                    try Task.checkCancellation()
                    guard uniqueInodes.insert("\(entry.identity.device):\(entry.identity.inode)").inserted else { continue }
                    do {
                        stream.yield(.progress("Verifying \((entry.path as NSString).lastPathComponent)"))
                        hashes[try files.hash(entry.path), default: []].append(entry)
                    } catch { stream.yield(.warning("Skipped duplicate candidate \(entry.path): \(error.localizedDescription)")) }
                }
                for (hash, group) in hashes where group.count > 1 {
                    let sorted = group.sorted { $0.path < $1.path }
                    let original = sorted[0].path
                    for entry in sorted {
                        let preserved = entry.path == original
                        let fresh = try files.entry(entry.path)
                        stream.yield(.finding(Finding(id: "duplicates:" + entry.path, moduleID: "duplicates", title: (entry.path as NSString).lastPathComponent, subtitle: entry.path, resource: .file(fresh.identity), bytes: entry.bytes, allocatedBytes: entry.allocated, modifiedAt: fresh.modified, details: [Detail("Preserved original", original), Detail("Group files", String(group.count)), Detail("SHA256", hash)], actions: preserved ? [] : [.trash], reason: preserved ? "This verified original is preserved; other copies can be reviewed." : "Contents match the preserved original. Both files are verified again before action.", blockedReason: preserved ? "One original is preserved in each group." : nil)))
                    }
                }
            }
        }
    }
}

public struct CacheModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "caches", name: "Caches & Logs", category: .developer, symbol: "archivebox", summary: "Known package caches and user logs. Review rebuild and offline-use costs.", usesRoots: false)
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            for (name, path) in KnownLocations.caches where FileManager.default.fileExists(atPath: path) && context.allows(path) {
                try Task.checkCancellation()
                stream.yield(.progress("Sizing \(name)"))
                if let entry = try? FileService().entry(path) {
                    stream.yield(.finding(try Streams.file(entry, module: "caches", title: name, reason: "Cached downloads may be needed for offline installs. Close the owning tools before moving this cache to Trash.", risk: .rebuild)))
                }
            }
            let logs = NSHomeDirectory() + "/Library/Logs"
            if FileManager.default.fileExists(atPath: logs) {
                for entry in Streams.children(logs, continuation: stream) where context.allows(entry.path) {
                    try Task.checkCancellation()
                    stream.yield(.finding(try Streams.file(entry, module: "caches", title: "Logs · " + (entry.path as NSString).lastPathComponent, reason: "Logs can help diagnose problems. Review before removal.")))
                }
            }
        }
    }
}

public struct DownloadsModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "downloads", name: "Downloads", category: .storage, symbol: "arrow.down.circle", summary: "Review downloaded files and installers; nothing is preselected.", usesRoots: false)
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            for entry in Streams.children(NSHomeDirectory() + "/Downloads", continuation: stream) where context.allows(entry.path) && !PathPolicy.protected(entry.path) {
                try Task.checkCancellation()
                stream.yield(.finding(try Streams.file(entry, module: "downloads", reason: "Downloaded file or folder. Inspect whether it is still needed before moving it to Trash.")))
            }
        }
    }
}

public struct TrashModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "trash", name: "Trash", category: .storage, symbol: "trash", summary: "Review individual Trash items before permanent removal.", usesRoots: false)
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            for entry in Streams.children(NSHomeDirectory() + "/.Trash", continuation: stream) where context.allows(entry.path) {
                try Task.checkCancellation()
                stream.yield(.finding(try Streams.file(entry, module: "trash", reason: "Already in Trash. Permanent removal cannot be undone.", actions: [.emptyTrash], risk: .permanent)))
            }
        }
    }
}

public struct ApplicationsModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "applications", name: "Applications", category: .applications, symbol: "app.badge", summary: "Installed apps and precisely named related files. Shared containers are preserved.", usesRoots: false)
    public init() {}
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            let running = await MainActor.run { Set(NSWorkspace.shared.runningApplications.compactMap { $0.bundleURL?.path }) }
            for root in ["/Applications", NSHomeDirectory() + "/Applications"] where FileManager.default.fileExists(atPath: root) {
                var apps: [FileEntry] = []
                try FileService().walk(context: ScanContext(roots: [root], exclusions: context.exclusions), visit: { entry in
                    if entry.isDirectory && entry.path.hasSuffix(".app") { apps.append(entry); return false }
                    return true
                }, warning: { stream.yield(.warning($0)) })
                for app in apps {
                    try Task.checkCancellation()
                    let info = (NSDictionary(contentsOfFile: app.path + "/Contents/Info.plist") as? [String: Any]) ?? [:]
                    let bundleID = info["CFBundleIdentifier"] as? String ?? ""
                    let version = info["CFBundleShortVersionString"] as? String ?? "Unknown"
                    let apple = bundleID.hasPrefix("com.apple."), active = running.contains(app.path)
                    let actions: [ActionKind] = apple || active || bundleID.isEmpty ? [] : [.trash]
                    var finding = try Streams.file(app, module: "applications", reason: "Moving an application to Trash does not remove all its data. Related files are separate selections.", actions: actions, details: [Detail("Bundle identifier", bundleID), Detail("Version", version), Detail("State", active ? "Running" : "Not running"), Detail("Application path", app.path)])
                    finding.blockedReason = active ? "Quit this application before removal." : apple ? "Apple applications are protected." : bundleID.isEmpty ? "Unknown app identity; inspect manually." : finding.blockedReason
                    if context.allows(app.path) { stream.yield(.finding(finding)) }
                    guard !bundleID.isEmpty, !apple else { continue }
                    let related = [
                        NSHomeDirectory() + "/Library/Caches/" + bundleID,
                        NSHomeDirectory() + "/Library/Preferences/" + bundleID + ".plist",
                        NSHomeDirectory() + "/Library/Saved Application State/" + bundleID + ".savedState",
                        NSHomeDirectory() + "/Library/Application Support/" + bundleID,
                    ]
                    for path in related where FileManager.default.fileExists(atPath: path) && context.allows(path) {
                        guard let entry = try? FileService().entry(path) else { continue }
                        var relatedFinding = try Streams.file(entry, module: "applications", title: (app.path as NSString).lastPathComponent + " · " + (path as NSString).lastPathComponent, reason: "Precisely named related data. May contain preferences or user documents; review it separately.", actions: active ? [] : [.trash], details: [Detail("Application path", app.path), Detail("Bundle identifier", bundleID)])
                        if active { relatedFinding.blockedReason = "Quit the owning application first." }
                        stream.yield(.finding(relatedFinding))
                    }
                }
            }
        }
    }
}

public struct AppLeftoversModule: ScanModule {
    public let descriptor = ModuleDescriptor(id: "leftovers", name: "App Leftovers", category: .applications, symbol: "app.dashed", summary: "Review precisely named files with no matching app in the usual install folders.", usesRoots: false)
    public init() {}
    public static func bundleIdentifier(from name: String) -> String? {
        let value = name.hasSuffix(".savedState") ? String(name.dropLast(11)) : name.hasSuffix(".plist") ? String(name.dropLast(6)) : name
        guard !value.hasPrefix("com.apple."), !value.hasPrefix("group."),
              value.range(of: #"^[A-Za-z0-9_-]+(\.[A-Za-z0-9_-]+){2,}$"#, options: .regularExpression) != nil else { return nil }
        return value
    }
    public func scan(in context: ScanContext) -> AsyncThrowingStream<ScanEvent, Error> {
        Streams.make { stream in
            var installed = Set<String>()
            for root in ["/Applications", NSHomeDirectory() + "/Applications"] where FileManager.default.fileExists(atPath: root) {
                try FileService().walk(context: ScanContext(roots: [root]), visit: { entry in
                    if entry.path.hasSuffix(".app") {
                        if let info = NSDictionary(contentsOfFile: entry.path + "/Contents/Info.plist"), let id = info["CFBundleIdentifier"] as? String { installed.insert(id) }
                        return false
                    }
                    return true
                }, warning: { stream.yield(.warning($0)) })
            }
            let active = await MainActor.run { Set(NSWorkspace.shared.runningApplications.compactMap(\.bundleIdentifier)) }
            for directory in ["Caches", "Preferences", "Saved Application State"] {
                let root = NSHomeDirectory() + "/Library/" + directory
                guard FileManager.default.fileExists(atPath: root) else { continue }
                for item in Streams.children(root, continuation: stream) where context.allows(item.path) {
                    try Task.checkCancellation()
                    guard let id = Self.bundleIdentifier(from: (item.path as NSString).lastPathComponent), !installed.contains(id) else { continue }
                    var finding = try Streams.file(item, module: "leftovers", reason: "No matching app was found in /Applications or ~/Applications. An app elsewhere may still use this file. Preferences may contain settings or license information; inspect before removal.", actions: active.contains(id) ? [] : [.trash], details: [Detail("Bundle identifier", id), Detail("Location type", directory), Detail("Classification", "Possible leftover; ownership is uncertain")])
                    if active.contains(id) { finding.blockedReason = "The owning application is running." }
                    stream.yield(.finding(finding))
                }
            }
        }
    }
}
