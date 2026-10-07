import AppKit
import SwiftUI

public enum Space {
    public static let xxs: CGFloat = 2, xs: CGFloat = 4, sm: CGFloat = 8, md: CGFloat = 12, lg: CGFloat = 16, xl: CGFloat = 24, xxl: CGFloat = 32
}
public enum Layout {
    public static let sidebar: CGFloat = 220, inspector: CGFloat = 320, inspectorMin: CGFloat = 240, inspectorMax: CGFloat = 420
    public static let contentMin: CGFloat = 400, windowWidth: CGFloat = 1120, windowHeight: CGFloat = 760, windowMinWidth: CGFloat = 880, windowMinHeight: CGFloat = 560
    public static let reviewWidth: CGFloat = 640, reviewHeight: CGFloat = 540, rowMinimum: CGFloat = 52, panelRadius: CGFloat = 16, controlRadius: CGFloat = 10, smallRadius: CGFloat = 5
    public static let iconSmall: CGFloat = 12, iconMedium: CGFloat = 16, iconLarge: CGFloat = 32, checkbox: CGFloat = 18
    public static let sidebarRow: CGFloat = 36, controlHeight: CGFloat = 40
}
/// System font (SF Pro) on a compact macOS scale.
public enum TypeStyle {
    public static let pageTitle = Font.system(size: 24, weight: .semibold)
    public static let title = Font.system(size: 17, weight: .semibold)
    public static let headline = Font.system(size: 15, weight: .semibold)
    public static let sectionTitle = Font.system(size: 13, weight: .semibold)
    public static let rowTitle = Font.system(size: 13, weight: .medium)
    public static let secondary = Font.system(size: 13)
    public static let body = Font.system(size: 12)
    public static let label = Font.system(size: 12, weight: .medium)
    public static let numeric = Font.system(size: 12).monospacedDigit()
    public static let caption = Font.system(size: 11)
    public static let captionEmphasis = Font.system(size: 11, weight: .medium)
    public static let metric = Font.system(size: 32, weight: .medium).monospacedDigit()
    public static let code = Font.system(size: 11, design: .monospaced)
}
/// Named Comfy palettes. Colors resolve at draw time, so switching applies on the next render.
public enum ComfyTheme: String, CaseIterable, Identifiable, Sendable {
    case walnut = "Paper & Walnut", honey = "Honey & Espresso", ember = "Ember", terracotta = "Terracotta & Navy", mauve = "Cream & Mauve", rose = "Linen & Rose"
    public var id: String { rawValue }
    nonisolated(unsafe) public static var current: ComfyTheme = .walnut

    /// Light and dark sRGB values, mirroring docs/brand/tokens.json.
    func hex(_ token: Palette.Token) -> (light: UInt32, dark: UInt32) {
        switch (self, token) {
        // Paper & Walnut: #FCFCFB paper with the brown ramp (docs/brand/tokens.json → ramps.brown).
        case (.walnut, .canvas): (0xFCFCFB, 0x161210)
        case (.walnut, .sidebar): (0xF6F3EF, 0x1D1814)
        case (.walnut, .sidebarSelection): (0xEBE2D8, 0x3A2C20)
        case (.walnut, .surface): (0xFFFFFF, 0x231D18)
        case (.walnut, .elevated): (0xFFFFFF, 0x2C241E)
        case (.walnut, .border): (0xE8E1D9, 0x3A3029)
        case (.walnut, .borderStrong): (0xCDBDAC, 0x52443A)
        case (.walnut, .track): (0xEFE9E2, 0x2C241E)
        case (.walnut, .accent): (0x6E5139, 0xC9A27E)
        case (.walnut, .onAccent): (0xFFFFFF, 0x1E150E)
        case (.walnut, .accentText): (0x6E5139, 0xD9BC9F)
        case (.walnut, .accentSymbol): (0x8A6B50, 0xC9A27E)
        case (.walnut, .selection): (0xF2ECE5, 0x3A2C20)
        case (.walnut, .ink): (0x261B12, 0xF3EEE8)
        case (.walnut, .muted): (0x6F6258, 0xB3A69A)
        case (.honey, .canvas): (0xFBF3E4, 0x1A130C)
        case (.honey, .sidebar): (0xF5E9D4, 0x211810)
        case (.honey, .sidebarSelection): (0xF2DCB8, 0x3D2A15)
        case (.honey, .surface): (0xFFFAF1, 0x281D12)
        case (.honey, .elevated): (0xFFFFFF, 0x33261A)
        case (.honey, .border): (0xEADBC2, 0x3E2F21)
        case (.honey, .borderStrong): (0xD9C2A0, 0x57432F)
        case (.honey, .track): (0xF2E3C9, 0x33261A)
        case (.honey, .accent): (0xE9B262, 0xE9B262)
        case (.honey, .onAccent): (0x281D12, 0x281D12)
        case (.honey, .accentText): (0x924716, 0xF0B47A)
        case (.honey, .accentSymbol): (0xB45E28, 0xE08850)
        case (.honey, .selection): (0xF7E5C4, 0x3D2A15)
        case (.honey, .ink): (0x281D12, 0xFBF3E4)
        case (.honey, .muted): (0x6E5E4C, 0xC2B29C)
        case (.ember, .canvas): (0xFAF7F6, 0x000000)
        case (.ember, .sidebar): (0xF3ECEA, 0x0E0B0B)
        case (.ember, .sidebarSelection): (0xF2D5D1, 0x3A100C)
        case (.ember, .surface): (0xFFFFFF, 0x151010)
        case (.ember, .elevated): (0xFFFFFF, 0x1F1716)
        case (.ember, .border): (0xE9DEDB, 0x2E2423)
        case (.ember, .borderStrong): (0xD4C2BE, 0x4A3533)
        case (.ember, .track): (0xF0E4E1, 0x261C1B)
        case (.ember, .accent): (0xAD2219, 0xC42A1E)
        case (.ember, .onAccent): (0xFFFFFF, 0xFFFFFF)
        case (.ember, .accentText): (0x7A160E, 0xFF7A6B)
        case (.ember, .accentSymbol): (0xE93324, 0xE93324)
        case (.ember, .selection): (0xFBE3DF, 0x3A100C)
        case (.ember, .ink): (0x1A1110, 0xF5EEED)
        case (.ember, .muted): (0x6B5B58, 0xB8A9A6)
        case (.ember, .destructive): (0x3D0A06, 0xFF8A7A)
        case (.ember, .onDestructive): (0xFFFFFF, 0x000000)
        case (.terracotta, .destructive): (0xA12D2A, 0xF57A96)
        case (.terracotta, .canvas): (0xF7F1E8, 0x241E1C)
        case (.terracotta, .sidebar): (0xF2EADF, 0x2C2523)
        case (.terracotta, .sidebarSelection): (0xEBD6CB, 0x4A3029)
        case (.terracotta, .surface): (0xFCF9F4, 0x332B29)
        case (.terracotta, .elevated): (0xFFFFFF, 0x403533)
        case (.terracotta, .border): (0xE6DCCD, 0x4D423F)
        case (.terracotta, .borderStrong): (0xD3C5B4, 0x615350)
        case (.terracotta, .track): (0xEBE1D3, 0x3A312F)
        case (.terracotta, .accent): (0xE07D63, 0xE8896F)
        case (.terracotta, .onAccent): (0x2A2220, 0x2A1A14)
        case (.terracotta, .accentText): (0x3B5A86, 0x9DB6DB)
        case (.terracotta, .accentSymbol): (0xC0583F, 0xEE9A82)
        case (.terracotta, .selection): (0xF6DED5, 0x4A3029)
        case (.terracotta, .ink): (0x403533, 0xF2EADF)
        case (.terracotta, .muted): (0x6E625E, 0xB9ABA2)
        case (.mauve, .canvas): (0xF8F4EC, 0x1F1A1A)
        case (.mauve, .sidebar): (0xEEE5D3, 0x282121)
        case (.mauve, .sidebarSelection): (0xE6D2CC, 0x4A3636)
        case (.mauve, .surface): (0xFCFAF5, 0x2D2626)
        case (.mauve, .elevated): (0xFFFFFF, 0x362E2E)
        case (.mauve, .border): (0xE6DCCD, 0x4A3F3E)
        case (.mauve, .borderStrong): (0xD5B6B3, 0x5E4E4D)
        case (.mauve, .track): (0xEEE5D3, 0x3A3030)
        case (.mauve, .accent): (0xBC9796, 0xC9A3A1)
        case (.mauve, .onAccent): (0x2B2621, 0x2A1B1B)
        case (.mauve, .accentText): (0x7A5C5A, 0xD9B8B5)
        case (.mauve, .accentSymbol): (0x937371, 0xD9B8B5)
        case (.mauve, .selection): (0xF0E2DF, 0x4A3636)
        case (.rose, .canvas): (0xF9F5F2, 0x1E1A17)
        case (.rose, .sidebar): (0xEDE3D5, 0x26211D)
        case (.rose, .sidebarSelection): (0xE4D2C8, 0x4A3530)
        case (.rose, .surface): (0xFDFBF9, 0x2B2622)
        case (.rose, .elevated): (0xFFFFFF, 0x332D28)
        case (.rose, .border): (0xE6DACB, 0x4A4038)
        case (.rose, .borderStrong): (0xD6C4B2, 0x5E5147)
        case (.rose, .track): (0xEDE3D5, 0x3A332D)
        case (.rose, .accent): (0xD0A097, 0xD9A79E)
        case (.rose, .onAccent): (0x2B2621, 0x2A1A16)
        case (.rose, .accentText), (.rose, .accentSymbol): (0x8E554B, 0xE2B3AA)
        case (.rose, .selection): (0xEFE0D9, 0x4A3530)
        case (_, .ink): (0x2B2621, 0xF1EBE1)
        case (_, .muted): (0x6B6157, 0xB3A99B)
        case (_, .leaf): (0x5EBEA5, 0x76CFB4)
        case (_, .warning): (0x7A520F, 0xEDC783)
        case (_, .destructive): (0xA12D2A, 0xF0857A)
        case (_, .onDestructive): (0xFFFBF5, 0x2A1810)
        case (_, .success): (0x3B7A5E, 0x86C9A8)
        }
    }
}

public enum Palette {
    enum Token: Sendable {
        case canvas, sidebar, sidebarSelection, surface, elevated, border, borderStrong, track
        case accent, onAccent, accentText, accentSymbol, selection
        case ink, muted, leaf, warning, destructive, onDestructive, success
    }
    public static let canvas = color(.canvas)
    public static let sidebar = color(.sidebar)
    public static let sidebarSelection = color(.sidebarSelection)
    public static let surface = color(.surface)
    public static let elevated = color(.elevated)
    public static let ink = color(.ink)
    public static let muted = color(.muted)
    /// Hairlines on cards and dividers.
    public static let border = color(.border)
    /// Outlines on controls.
    public static let borderStrong = color(.borderStrong)
    public static let track = color(.track)
    /// Fills: primary buttons, bars and highlights. Pair with `onAccent` content.
    public static let accent = color(.accent)
    public static let onAccent = color(.onAccent)
    /// Accent-colored text on cream surfaces (AA on every background).
    public static let accentText = color(.accentText)
    /// Accent-colored symbols and non-text marks.
    public static let accentSymbol = color(.accentSymbol)
    public static let selection = color(.selection)
    public static let leaf = color(.leaf)
    public static let warning = color(.warning)
    public static let destructive = color(.destructive)
    public static let onDestructive = color(.onDestructive)
    public static let success = color(.success)

    private static func color(_ token: Token) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let pair = ComfyTheme.current.hex(token)
            let hex = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? pair.dark : pair.light
            return NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
        })
    }
}
public enum Stroke {
    public static let hairline: CGFloat = 1, focus: CGFloat = 2
}
public enum Opacity {
    public static let disabled = 0.45, pressed = 0.8
}
public enum ButtonKind: Equatable { case primary, secondary, destructive }

public struct ActionButton: View {
    private let title: String
    private let kind: ButtonKind
    private let disabled: Bool
    private let action: () -> Void
    public init(_ title: String, kind: ButtonKind = .secondary, disabled: Bool = false, action: @escaping () -> Void) {
        self.title = title; self.kind = kind; self.disabled = disabled; self.action = action
    }
    public var body: some View {
        Button(title, role: kind == .destructive ? .destructive : nil, action: action)
            .buttonStyle(ComfyButtonStyle(kind: kind)).disabled(disabled)
    }
}

struct ComfyButtonStyle: ButtonStyle {
    let kind: ButtonKind
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(TypeStyle.body.weight(.medium))
            .foregroundStyle(foreground)
            .padding(.horizontal, Space.lg).padding(.vertical, Space.sm - Space.xxs)
            .background(fill, in: RoundedRectangle(cornerRadius: Layout.controlRadius))
            .overlay(RoundedRectangle(cornerRadius: Layout.controlRadius).strokeBorder(kind == .secondary ? Palette.borderStrong : .clear, lineWidth: Stroke.hairline))
            .opacity(enabled ? (configuration.isPressed ? Opacity.pressed : 1) : Opacity.disabled)
            .contentShape(RoundedRectangle(cornerRadius: Layout.controlRadius))
    }
    private var fill: Color {
        switch kind { case .primary: Palette.accent; case .secondary: Palette.surface; case .destructive: Palette.destructive }
    }
    private var foreground: Color {
        switch kind { case .primary: Palette.onAccent; case .secondary: Palette.ink; case .destructive: Palette.onDestructive }
    }
}

public struct Panel<Content: View>: View {
    private let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public var body: some View {
        content.padding(Space.lg).frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Layout.panelRadius))
            .overlay(RoundedRectangle(cornerRadius: Layout.panelRadius).strokeBorder(Palette.border, lineWidth: Stroke.hairline))
            .shadow(color: Palette.ink.opacity(0.06), radius: Space.sm, y: Space.xxs)
    }
}

/// A compact figure for summaries under search fields and toolbars.
public struct StatChip: View {
    private let label: String, value: String, emphasized: Bool
    public init(_ label: String, value: String, emphasized: Bool = false) { self.label = label; self.value = value; self.emphasized = emphasized }
    public var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            Text(value).font(TypeStyle.headline).monospacedDigit().foregroundStyle(emphasized ? Palette.accentText : Palette.ink).lineLimit(1)
            Text(label).font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1)
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.sm)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: Layout.controlRadius))
        .overlay(RoundedRectangle(cornerRadius: Layout.controlRadius).strokeBorder(Palette.border, lineWidth: Stroke.hairline))
        .accessibilityElement(children: .combine)
    }
}

public struct MetricTile: View {
    private let title: String, value: String, detail: String
    public init(_ title: String, value: String, detail: String) { self.title = title; self.value = value; self.detail = detail }
    public var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.sm) {
                Text(title).font(TypeStyle.caption).foregroundStyle(.secondary)
                Text(value).font(TypeStyle.metric)
                Text(detail).font(TypeStyle.caption).foregroundStyle(.secondary)
            }
        }.accessibilityElement(children: .combine)
    }
}

public struct StatusBadge: View {
    private let title: String
    private let warning: Bool
    public init(_ title: String, warning: Bool = false) { self.title = title; self.warning = warning }
    public var body: some View {
        Label(title, systemImage: warning ? "exclamationmark.circle" : "info.circle")
            .font(TypeStyle.caption).foregroundStyle(warning ? Palette.warning : .secondary)
            .padding(.horizontal, Space.sm).padding(.vertical, Space.xs)
            .background(Palette.surface, in: Capsule())
    }
}

public struct EmptyState: View {
    private let title: String, message: String, symbol: String
    public init(_ title: String, message: String, symbol: String = "tray") { self.title = title; self.message = message; self.symbol = symbol }
    public var body: some View {
        VStack(spacing: Space.lg) {
            Image(systemName: symbol).font(TypeStyle.metric).foregroundStyle(.secondary)
            Text(title).font(TypeStyle.sectionTitle)
            Text(message).font(TypeStyle.secondary).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }.padding(Space.xxl).frame(maxWidth: .infinity, maxHeight: .infinity).accessibilityElement(children: .combine)
    }
}

public struct FindingRow: View {
    private let title: String, subtitle: String, value: String, badge: String, symbol: String
    private let active: Bool, eligible: Bool
    @Binding private var checked: Bool
    private let inspect: () -> Void
    public init(title: String, subtitle: String, value: String, badge: String, symbol: String, active: Bool, eligible: Bool, checked: Binding<Bool>, inspect: @escaping () -> Void) {
        self.title = title; self.subtitle = subtitle; self.value = value; self.badge = badge; self.symbol = symbol
        self.active = active; self.eligible = eligible; _checked = checked; self.inspect = inspect
    }
    public var body: some View {
        HStack(spacing: Space.md) {
            Toggle("Select \(title) for review", isOn: $checked).labelsHidden().toggleStyle(.checkbox).disabled(!eligible)
            Button(action: inspect) {
                HStack(spacing: Space.md) {
                    Image(systemName: symbol).foregroundStyle(Palette.accentSymbol)
                    VStack(alignment: .leading, spacing: Space.xs) {
                        Text(title).font(TypeStyle.rowTitle).lineLimit(1)
                        Text(subtitle).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: Space.xs) {
                        Text(value).font(TypeStyle.numeric)
                        Text(badge).font(TypeStyle.caption).foregroundStyle(.secondary)
                    }
                }.contentShape(Rectangle())
            }.buttonStyle(.plain).accessibilityLabel("Inspect \(title), \(value), \(badge)")
        }
        .padding(.horizontal, Space.lg).padding(.vertical, Space.sm).frame(minHeight: Layout.rowMinimum)
        .background(active ? Palette.selection : .clear)
    }
}

public struct KeyValueRow: View {
    private let label: String, value: String
    public init(_ label: String, _ value: String) { self.label = label; self.value = value }
    public var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(label).font(TypeStyle.caption).foregroundStyle(.secondary)
            Text(value).font(TypeStyle.secondary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
        }.frame(maxWidth: .infinity, alignment: .leading)
    }
}

public struct PageHeader: View {
    private let title: String, subtitle: String
    public init(_ title: String, subtitle: String) { self.title = title; self.subtitle = subtitle }
    public var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            Text(title).font(TypeStyle.pageTitle)
            Text(subtitle).font(TypeStyle.secondary).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }.frame(maxWidth: .infinity, alignment: .leading)
    }
}

public struct StorageBar: View {
    private let title: String, value: String
    private let fraction: Double
    public init(_ title: String, value: String, fraction: Double) { self.title = title; self.value = value; self.fraction = fraction }
    public var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            HStack { Text(title).lineLimit(1); Spacer(); Text(value).monospacedDigit() }.font(TypeStyle.caption)
            GeometryReader { geometry in
                Capsule().fill(Palette.track)
                    .overlay(alignment: .leading) { Capsule().fill(Palette.accent).frame(width: geometry.size.width * min(max(fraction, 0), 1)) }
            }.frame(height: Space.sm)
        }.accessibilityElement(children: .combine)
    }
}

public struct ComponentGallery: View {
    @State private var checked = false
    public init() {}
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xl) {
                PageHeader("Component Gallery", subtitle: "The shared vocabulary used by every module. Fixture data only.")
                HStack {
                    MetricTile("Logical size", value: "12.4 GB", detail: "Allocated size may differ")
                    MetricTile("Processes", value: "3", detail: "Current-user orphan candidates")
                }
                Panel {
                    HStack { ActionButton("Scan", kind: .primary) {}; ActionButton("Cancel") {}; ActionButton("Terminate", kind: .destructive) {}; ActionButton("Unavailable", disabled: true) {} }
                }
                HStack(spacing: Space.sm) { StatChip("Items", value: "128"); StatChip("Total size", value: "12.4 GB"); StatChip("Ready to review", value: "9.1 GB", emphasized: true); StatChip("Needs inspection", value: "3") }
                Panel { HStack { StatusBadge("Complete"); StatusBadge("Partial", warning: true); StatusBadge("Permanent", warning: true) } }
                Panel { StorageBar("Developer data", value: "12.4 GB", fraction: 0.6) }
                Panel {
                    VStack(spacing: Space.xs) {
                        FindingRow(title: "frontend", subtitle: "~/Projects/frontend/node_modules", value: "1.8 GB", badge: "Rebuild required", symbol: "shippingbox", active: false, eligible: true, checked: $checked) {}
                        FindingRow(title: "node", subtitle: "~/Projects/frontend", value: "53% CPU · 240 MB", badge: "Review", symbol: "cpu", active: true, eligible: true, checked: $checked) {}
                        FindingRow(title: "Main worktree", subtitle: "~/Projects/frontend", value: "2.1 GB", badge: "Protected", symbol: "arrow.triangle.branch", active: false, eligible: false, checked: .constant(false)) {}
                    }
                }
                Panel { VStack(alignment: .leading, spacing: Space.lg) { KeyValueRow("State", "Incomplete scan · access denied"); KeyValueRow("Next action", "Choose a readable folder or grant access in System Settings.") } }
                EmptyState("No results", message: "Choose roots, then scan. A partial scan does not prove there is no clutter.")
            }.padding(Space.xl)
        }.background(Palette.canvas)
    }
}
