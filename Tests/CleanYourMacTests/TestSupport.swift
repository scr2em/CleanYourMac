import Foundation

struct Fixture {
    let url: URL
    init() throws {
        url = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent(".test-fixtures/" + UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    }
    var path: String { url.resolvingSymlinksInPath().path }
    @discardableResult
    func write(_ name: String, _ contents: String = "fixture") throws -> String {
        let target = url.appendingPathComponent(name)
        try FileManager.default.createDirectory(at: target.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(contents.utf8).write(to: target)
        return target.resolvingSymlinksInPath().path
    }
    func clean() { try? FileManager.default.removeItem(at: url) }
}
