import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Ecosystem logos: the single-colour SVGs in the bundled `BrandIcons` folder, named by slug,
/// with the brand colour as each file's `fill`. Read and parsed once, on first use.
public enum BrandCatalog {
    /// Inside the app's Resources when packaged; SwiftPM's resource bundle when run from a build.
    static let folder: URL? = {
        if let url = Bundle.main.url(forResource: "CleanYourMac_CleanYourMacDesignSystem", withExtension: "bundle"),
           let folder = Bundle(url: url)?.url(forResource: "BrandIcons", withExtension: nil) {
            return folder
        }
        return Bundle.module.url(forResource: "BrandIcons", withExtension: nil)
    }()
    static let icons: [String: SVGIcon] = {
        guard let folder, let files = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil) else { return [:] }
        var icons: [String: SVGIcon] = [:]
        for file in files where file.pathExtension == "svg" {
            if let text = try? String(contentsOf: file, encoding: .utf8), let icon = SVGIcon(svg: text) {
                icons[file.deletingPathExtension().lastPathComponent] = icon
            }
        }
        return icons
    }()
    /// Each logo as a path in a 24×24 box.
    static let paths: [String: Path] = icons.mapValues(path)

    public static var slugs: [String] { icons.keys.sorted() }
    /// Whether a logo exists for `slug`.
    public static func contains(_ slug: String) -> Bool { icons[slug] != nil }
    public static func name(_ slug: String) -> String? { icons[slug]?.title }
    static func hex(_ slug: String) -> UInt32? { icons[slug]?.fill }

    static func path(_ icon: SVGIcon) -> Path {
        var path = Path()
        for command in icon.commands {
            switch command {
            case let .move(x, y): path.move(to: CGPoint(x: x, y: y))
            case let .line(x, y): path.addLine(to: CGPoint(x: x, y: y))
            case let .cubic(x1, y1, x2, y2, x, y): path.addCurve(to: CGPoint(x: x, y: y), control1: CGPoint(x: x1, y: y1), control2: CGPoint(x: x2, y: y2))
            case let .quad(x1, y1, x, y): path.addQuadCurve(to: CGPoint(x: x, y: y), control: CGPoint(x: x1, y: y1))
            case .close: path.closeSubpath()
            }
        }
        // Fit any view box into the 24-point square the shape scales from.
        let box = icon.viewBox
        let scale = 24 / max(box.width, box.height, 1)
        return path.applying(CGAffineTransform(scaleX: scale, y: scale).translatedBy(x: -box.x, y: -box.y))
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
