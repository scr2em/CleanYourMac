import CleanYourMacCore
import CleanYourMacModules
import CleanYourMacPlatform
import Foundation

@main enum CLI {
    static func main() async {
        let registry = BuiltInModules.registry(processes: ProcessService())
        let arguments = Array(CommandLine.arguments.dropFirst())
        if arguments.isEmpty || arguments.contains("--help") {
            print("""
            CleanYourMac · independent Swift macOS tools

            cym modules
            cym scan MODULE [ROOT ...] [--json]

            Scans are read-only. Reviewed actions are available in the native app.
            JSON exports omit process commands and abbreviate the home directory.
            """)
            return
        }
        if arguments[0] == "modules" {
            for module in registry.modules { print("\(module.descriptor.id)\t\(module.descriptor.name)") }
            return
        }
        guard arguments[0] == "scan", arguments.count >= 2, let module = registry.module(arguments[1]) else {
            FileHandle.standardError.write(Data("Unknown command or module. Use cym modules or cym --help.\n".utf8)); exit(2)
        }
        guard arguments.dropFirst(2).filter({ $0.hasPrefix("--") }).allSatisfy({ $0 == "--json" }) else {
            FileHandle.standardError.write(Data("Unknown option. Supported scan option: --json.\n".utf8)); exit(2)
        }
        let roots = arguments.dropFirst(2).filter { $0 != "--json" }
        guard !module.descriptor.usesRoots || !roots.isEmpty || !KnownLocations.projectRoots.isEmpty else {
            FileHandle.standardError.write(Data("Choose at least one folder: cym scan \(module.descriptor.id) /path/to/folder.\n".utf8)); exit(2)
        }
        let context = ScanContext(roots: roots.isEmpty ? KnownLocations.projectRoots : roots.map(PathPolicy.canonical))
        var findings: [Finding] = [], warnings: [String] = []
        do {
            for try await event in module.scan(in: context) {
                switch event {
                case .finding(var finding):
                    finding.details.removeAll { $0.label == "Command" }
                    findings.append(finding)
                case .warning(let warning): warnings.append(warning)
                case .progress: break
                }
            }
            if arguments.contains("--json") {
                struct Report: Encodable { let findings: [Finding]; let warnings: [String]; let complete: Bool }
                let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
                let data = try encoder.encode(Report(findings: findings, warnings: warnings, complete: warnings.isEmpty))
                print(String(decoding: data, as: UTF8.self).replacingOccurrences(of: NSHomeDirectory(), with: "~"))
            } else {
                for finding in findings {
                    let size = finding.bytes.map { ByteCountFormatter.string(fromByteCount: Int64(clamping: $0), countStyle: .file) } ?? "—"
                    print("\(size)\t\(finding.title)\t\(finding.subtitle.replacingOccurrences(of: NSHomeDirectory(), with: "~"))")
                }
                for warning in warnings { FileHandle.standardError.write(Data("Warning: \(warning)\n".utf8)) }
                print("\(findings.count) findings. \(warnings.isEmpty ? "Scan complete." : "Partial coverage.")")
            }
            if !warnings.isEmpty { exit(3) }
        } catch { FileHandle.standardError.write(Data("Scan failed: \(error.localizedDescription)\n".utf8)); exit(1) }
    }
}
