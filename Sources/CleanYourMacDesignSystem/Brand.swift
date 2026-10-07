import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Ecosystem logos (from Simple Icons), parsed once into paths in a 24×24 box.
public enum BrandCatalog {
    static let paths: [String: Path] = icons.mapValues { parse($0.path) }

    /// Whether a logo exists for `slug`.
    public static func contains(_ slug: String) -> Bool { icons[slug] != nil }
    public static func name(_ slug: String) -> String? { icons[slug]?.name }
    static func hex(_ slug: String) -> UInt32? { icons[slug]?.hex }

    /// Reads the generated absolute M, L, C, Q and Z commands.
    static func parse(_ text: String) -> Path {
        var path = Path()
        var command: Character?
        var values: [CGFloat] = []
        func point(_ i: Int) -> CGPoint { CGPoint(x: values[i], y: values[i + 1]) }
        func flush() {
            switch command {
            case "M" where values.count >= 2: path.move(to: point(0))
            case "L" where values.count >= 2: path.addLine(to: point(0))
            case "C" where values.count >= 6: path.addCurve(to: point(4), control1: point(0), control2: point(2))
            case "Q" where values.count >= 4: path.addQuadCurve(to: point(2), control: point(0))
            case "Z": path.closeSubpath()
            default: break
            }
        }
        for token in text.split(separator: " ") {
            guard let first = token.first else { continue }
            if first.isLetter {
                flush()
                command = first
                values = []
                if let value = Double(token.dropFirst()) { values.append(CGFloat(value)) }
            } else if let value = Double(token) {
                values.append(CGFloat(value))
            }
        }
        flush()
        return path
    }
}

/// An ecosystem's logo filled with its brand colour. Near-black and near-white marks use the
/// ink colour instead, so they stay visible in light and dark appearances.
public struct BrandIcon: View {
    private let slug: String
    @Environment(\.colorScheme) private var scheme
    public init?(_ slug: String?) {
        guard let slug, BrandCatalog.contains(slug) else { return nil }
        self.slug = slug
    }
    public var body: some View {
        BrandShape(slug: slug)
            .fill(color)
            .aspectRatio(1, contentMode: .fit)
            .accessibilityLabel(BrandCatalog.name(slug) ?? slug)
    }
    private var color: Color {
        guard let hex = BrandCatalog.hex(slug) else { return Palette.ink }
        let red = Double((hex >> 16) & 0xFF) / 255, green = Double((hex >> 8) & 0xFF) / 255, blue = Double(hex & 0xFF) / 255
        let luminance = 0.2126 * red + 0.7152 * green + 0.0722 * blue
        let unreadable = scheme == .dark ? luminance < 0.22 : luminance > 0.88
        if unreadable || luminance < 0.08 || luminance > 0.95 { return Palette.ink }
        return Color(.sRGB, red: red, green: green, blue: blue)
    }
}

/// A logo path scaled to fit its frame.
struct BrandShape: Shape {
    let slug: String
    func path(in rect: CGRect) -> Path {
        guard let path = BrandCatalog.paths[slug] else { return Path() }
        let scale = min(rect.width, rect.height) / 24
        let transform = CGAffineTransform(translationX: rect.midX - 12 * scale, y: rect.midY - 12 * scale)
            .scaledBy(x: scale, y: scale)
        return path.applying(transform)
    }
}

/// What a result row shows before its title.
public enum RowIcon {
    /// An SF Symbol in the accent colour.
    case symbol(String)
    /// An ecosystem logo in its brand colour.
    case brand(String)
    /// A file, folder or app icon as Finder shows it.
    case file(NSImage)
}

/// Finder icons for result paths, cached by extension (or by path for apps and folders).
@MainActor public enum FileIcons {
    private static let cache = NSCache<NSString, NSImage>()
    public static func icon(for path: String) -> NSImage {
        let url = URL(fileURLWithPath: path)
        let ext = url.pathExtension.lowercased()
        // Apps and bundles have their own icons; plain files share one per type.
        let key = (ext.isEmpty || ["app", "bundle", "framework", "xcodeproj", "xcworkspace", "pkg", "dmg"].contains(ext)) ? path : "." + ext
        if let cached = cache.object(forKey: key as NSString) { return cached }
        let image = key == path ? NSWorkspace.shared.icon(forFile: path) : NSWorkspace.shared.icon(for: UTType(filenameExtension: ext) ?? .data)
        cache.setObject(image, forKey: key as NSString)
        return image
    }
}
