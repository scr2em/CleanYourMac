import AppKit
import SwiftUI

public enum Space {
    public static let xxs: CGFloat = 2, xs: CGFloat = 4, sm: CGFloat = 8, md: CGFloat = 12, lg: CGFloat = 16, xl: CGFloat = 24, xxl: CGFloat = 32
}
public enum Layout {
    public static let sidebar: CGFloat = 220, inspector: CGFloat = 320, inspectorMin: CGFloat = 240, inspectorMax: CGFloat = 420
    public static let contentMin: CGFloat = 400, windowWidth: CGFloat = 1120, windowHeight: CGFloat = 760, windowMinWidth: CGFloat = 880, windowMinHeight: CGFloat = 560
    /// Soft Studio radii: big, friendly corners on surfaces; pills for controls.
    public static let reviewWidth: CGFloat = 640, reviewHeight: CGFloat = 540, rowMinimum: CGFloat = 56, panelRadius: CGFloat = 20, controlRadius: CGFloat = 14, smallRadius: CGFloat = 8
    public static let rowRadius: CGFloat = 12, sheetRadius: CGFloat = 22, tileRadius: CGFloat = 10
    public static let iconSmall: CGFloat = 12, iconMedium: CGFloat = 16, iconLarge: CGFloat = 32, rowIcon: CGFloat = 24, rowTile: CGFloat = 34, checkbox: CGFloat = 20
    public static let sidebarRow: CGFloat = 36, sidebarTile: CGFloat = 26, controlHeight: CGFloat = 34
    public static let chipHeight: CGFloat = 30, toolIcon: CGFloat = 44, calloutMaxHeight: CGFloat = 180
    public static let scanOrb: CGFloat = 200, scanOrbCompact: CGFloat = 64, diskBar: CGFloat = 24
}
/// Soft Studio type: SF Pro Rounded for titles, labels and figures, which carry the app's
/// voice; regular SF Pro for running text, which stays easiest to read.
public enum TypeStyle {
    public static let pageTitle = Font.system(size: 26, weight: .bold, design: .rounded)
    public static let title = Font.system(size: 18, weight: .semibold, design: .rounded)
    public static let headline = Font.system(size: 15, weight: .semibold, design: .rounded)
    public static let sectionTitle = Font.system(size: 13, weight: .semibold, design: .rounded)
    public static let rowTitle = Font.system(size: 13, weight: .semibold, design: .rounded)
    public static let secondary = Font.system(size: 13)
    public static let body = Font.system(size: 12)
    public static let label = Font.system(size: 12, weight: .semibold, design: .rounded)
    public static let numeric = Font.system(size: 13, weight: .semibold, design: .rounded).monospacedDigit()
    public static let caption = Font.system(size: 11)
    public static let captionEmphasis = Font.system(size: 11, weight: .semibold, design: .rounded)
    public static let metric = Font.system(size: 34, weight: .bold, design: .rounded).monospacedDigit()
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
extension ComfyTheme {
    /// A colour from this palette, whichever palette is current, for previews.
    func swatch(_ token: Palette.Token) -> Color {
        let pair = hex(token)
        return Color(nsColor: NSColor(name: nil) { appearance in
            let hex = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? pair.dark : pair.light
            return NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
        })
    }
}
/// Overlapping dots previewing a palette's canvas, selection and accents.
public struct ThemeSwatch: View {
    private let theme: ComfyTheme
    public init(_ theme: ComfyTheme) { self.theme = theme }
    public var body: some View {
        HStack(spacing: -Space.xs) {
            ForEach([Palette.Token.canvas, .sidebarSelection, .accentSymbol, .accent], id: \.self) { token in
                Circle()
                    .fill(theme.swatch(token))
                    .overlay(Circle().strokeBorder(Palette.borderStrong, lineWidth: Stroke.hairline))
                    .frame(width: Layout.iconMedium, height: Layout.iconMedium)
            }
        }
        .accessibilityHidden(true)
    }
}
/// Shared animation timing: quick for hover and focus, springy for presses and layout.
public enum Motion {
    public static let quick = Animation.easeOut(duration: 0.15)
    public static let press = Animation.spring(duration: 0.25, bounce: 0.35)
}

public extension View {
    /// Soft Studio elevation: a tight contact shadow under a wide, faint one.
    func softShadow(_ level: Double = 1) -> some View {
        shadow(color: Palette.ink.opacity(0.05 * level), radius: 1, y: 1)
            .shadow(color: Palette.ink.opacity(0.07 * level), radius: 14 * level, y: 6 * level)
    }
    /// A raised surface: filled, rounded, hairline-bordered and softly shadowed.
    func card(radius: CGFloat = Layout.panelRadius, fill: Color = Palette.surface, elevation: Double = 1) -> some View {
        background(fill, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(Palette.border, lineWidth: Stroke.hairline))
            .softShadow(elevation)
    }
}
public enum Stroke {
    public static let hairline: CGFloat = 1, focus: CGFloat = 2
}
public enum Opacity {
    public static let disabled = 0.45, pressed = 0.8
}
public enum ButtonKind: Equatable { case primary, secondary, destructive }

public extension View {
    /// Disables the view and, while it is disabled, says why on hover (and to VoiceOver).
    /// Disabled controls do not reliably show their own tooltips, so a transparent layer
    /// carries the explanation.
    func disabled(_ disabled: Bool, because reason: String?) -> some View {
        self.disabled(disabled).overlay {
            if disabled, let reason, !reason.isEmpty {
                Color.clear.contentShape(Rectangle()).help(reason).accessibilityLabel(reason)
            }
        }
    }
}

public struct ActionButton: View {
    private let title: String
    private let kind: ButtonKind
    private let disabled: Bool
    private let reason: String?
    private let action: () -> Void
    /// `reason` explains on hover why the button is disabled.
    public init(_ title: String, kind: ButtonKind = .secondary, disabled: Bool = false, reason: String? = nil, action: @escaping () -> Void) {
        self.title = title; self.kind = kind; self.disabled = disabled; self.reason = reason; self.action = action
    }
    public var body: some View {
        Button(title, role: kind == .destructive ? .destructive : nil, action: action)
            .buttonStyle(ComfyButtonStyle(kind: kind)).disabled(disabled, because: reason)
    }
}

struct ComfyButtonStyle: ButtonStyle {
    let kind: ButtonKind
    @Environment(\.isEnabled) private var enabled
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(TypeStyle.label)
            .foregroundStyle(foreground)
            .padding(.horizontal, Space.lg + Space.xxs).frame(minHeight: Layout.controlHeight)
            .background(fill, in: Capsule())
            // A soft top highlight gives filled buttons some body.
            .overlay(Capsule().fill(LinearGradient(colors: [.white.opacity(kind == .secondary ? 0 : 0.18), .clear], startPoint: .top, endPoint: .center)))
            .overlay(Capsule().strokeBorder(kind == .secondary ? Palette.borderStrong : .clear, lineWidth: Stroke.hairline))
            .softShadow(kind == .secondary ? 0.4 : 0.7)
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
            .animation(Motion.press, value: configuration.isPressed)
            .opacity(enabled ? 1 : Opacity.disabled)
            .contentShape(Capsule())
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
        content.padding(Space.lg + Space.xxs).frame(maxWidth: .infinity, alignment: .leading)
            .card()
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
        .card(radius: Layout.controlRadius, elevation: 0.4)
        .accessibilityElement(children: .combine)
    }
}

public struct MetricTile: View {
    private let title: String, value: String, detail: String
    public init(_ title: String, value: String, detail: String) { self.title = title; self.value = value; self.detail = detail }
    public var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.sm) {
                Text(title).font(TypeStyle.caption).foregroundStyle(Palette.muted)
                Text(value).font(TypeStyle.metric)
                Text(detail).font(TypeStyle.caption).foregroundStyle(Palette.muted)
            }
        }.accessibilityElement(children: .combine)
    }
}

public struct StatusBadge: View {
    private let title: String
    private let warning: Bool
    public init(_ title: String, warning: Bool = false) { self.title = title; self.warning = warning }
    public var body: some View {
        Label(title, systemImage: warning ? "exclamationmark.circle.fill" : "info.circle.fill")
            .font(TypeStyle.captionEmphasis).foregroundStyle(warning ? Palette.warning : Palette.accentText)
            .padding(.horizontal, Space.sm).padding(.vertical, Space.xxs + 1)
            .background((warning ? Palette.warning : Palette.accentSymbol).opacity(0.12), in: Capsule())
    }
}

public struct EmptyState: View {
    private let title: String, message: String, symbol: String
    public init(_ title: String, message: String, symbol: String = "tray") { self.title = title; self.message = message; self.symbol = symbol }
    public var body: some View {
        VStack(spacing: Space.lg) {
            Image(systemName: symbol).font(TypeStyle.metric).foregroundStyle(Palette.muted)
            Text(title).font(TypeStyle.sectionTitle)
            Text(message).font(TypeStyle.secondary).foregroundStyle(Palette.muted).multilineTextAlignment(.center)
        }.padding(Space.xxl).frame(maxWidth: .infinity, maxHeight: .infinity).accessibilityElement(children: .combine)
    }
}

public struct FindingRow: View {
    private let title: String, subtitle: String, value: String, badge: String, icon: RowIcon
    private let active: Bool, eligible: Bool
    private let disabledReason: String?
    @Binding private var checked: Bool
    private let inspect: () -> Void
    /// `disabledReason` explains on hover why the checkbox cannot be ticked.
    public init(title: String, subtitle: String, value: String, badge: String, icon: RowIcon, active: Bool, eligible: Bool, disabledReason: String? = nil, checked: Binding<Bool>, inspect: @escaping () -> Void) {
        self.title = title; self.subtitle = subtitle; self.value = value; self.badge = badge; self.icon = icon
        self.active = active; self.eligible = eligible; self.disabledReason = disabledReason; _checked = checked; self.inspect = inspect
    }
    public init(title: String, subtitle: String, value: String, badge: String, symbol: String, active: Bool, eligible: Bool, checked: Binding<Bool>, inspect: @escaping () -> Void) {
        self.init(title: title, subtitle: subtitle, value: value, badge: badge, icon: .symbol(symbol), active: active, eligible: eligible, checked: checked, inspect: inspect)
    }
    public var body: some View {
        HStack(spacing: Space.md) {
            Toggle("Select \(title) for review", isOn: $checked).labelsHidden().toggleStyle(SoftCheckboxStyle())
                .disabled(!eligible, because: disabledReason)
            Button(action: inspect) {
                HStack(spacing: Space.md) {
                    IconTile(icon: icon)
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        Text(title).font(TypeStyle.rowTitle).foregroundStyle(Palette.ink).lineLimit(1)
                        Text(subtitle).font(TypeStyle.caption).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.middle)
                    }
                    Spacer(minLength: Space.md)
                    VStack(alignment: .trailing, spacing: Space.xs) {
                        Text(value).font(TypeStyle.numeric).foregroundStyle(Palette.ink)
                        if !badge.isEmpty { Pill(badge) }
                    }
                }.contentShape(Rectangle())
            }.buttonStyle(.plain).accessibilityLabel("Inspect \(title), \(value), \(badge)")
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.xs).frame(minHeight: Layout.rowMinimum)
        .background(active ? Palette.selection : .clear, in: RoundedRectangle(cornerRadius: Layout.rowRadius, style: .continuous))
        .padding(.horizontal, Space.sm)
    }
}

/// A row's leading icon in a fixed square, so titles align whatever the icon kind.
public struct RowIconView: View {
    let icon: RowIcon
    public init(icon: RowIcon) { self.icon = icon }
    public var body: some View {
        Group {
            switch icon {
            case .symbol(let name):
                Image(systemName: name).font(TypeStyle.headline).foregroundStyle(Palette.accentSymbol)
            case .brand(let slug):
                if let brand = BrandIcon(slug) {
                    brand.padding(Space.xxs)
                } else {
                    Image(systemName: "shippingbox").font(TypeStyle.headline).foregroundStyle(Palette.accentSymbol)
                }
            case .file(let image):
                Image(nsImage: image).resizable().interpolation(.high).aspectRatio(contentMode: .fit)
            }
        }
        .frame(width: Layout.rowIcon, height: Layout.rowIcon)
        .accessibilityHidden(true)
    }
}

public struct KeyValueRow: View {
    private let label: String, value: String
    public init(_ label: String, _ value: String) { self.label = label; self.value = value }
    public var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(label).font(TypeStyle.caption).foregroundStyle(Palette.muted)
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
            Text(subtitle).font(TypeStyle.secondary).foregroundStyle(Palette.muted).fixedSize(horizontal: false, vertical: true)
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

/// A disk's capacity split into labelled segments, like Disk Utility's bar, with a legend.
public struct DiskBar: View {
    public enum Style: Sendable { case ready, found, used, purgeable, free }
    public struct Segment: Identifiable, Equatable, Sendable {
        public let id: String
        public let label: String
        public let bytes: UInt64
        public let style: Style
        public init(_ label: String, bytes: UInt64, style: Style) { id = label; self.label = label; self.bytes = bytes; self.style = style }
    }
    private let segments: [Segment]
    private let format: (UInt64) -> String
    public init(_ segments: [Segment], format: @escaping (UInt64) -> String) { self.segments = segments; self.format = format }

    private var total: Double { Double(segments.reduce(0) { $0 + $1.bytes }) }
    private var shown: [Segment] { segments.filter { $0.bytes > 0 || $0.style == .free } }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            GeometryReader { geometry in
                HStack(spacing: 0) {
                    ForEach(shown) { segment in
                        Rectangle()
                            .fill(fill(segment.style))
                            .frame(width: total > 0 ? geometry.size.width * Double(segment.bytes) / total : 0)
                    }
                }
            }
            .frame(height: Layout.diskBar)
            .background(Palette.surface)
            .clipShape(RoundedRectangle(cornerRadius: Layout.smallRadius))
            .overlay(RoundedRectangle(cornerRadius: Layout.smallRadius).strokeBorder(Palette.borderStrong, lineWidth: Stroke.hairline))
            HStack(alignment: .top, spacing: Space.lg) {
                ForEach(shown) { segment in
                    HStack(alignment: .firstTextBaseline, spacing: Space.sm) {
                        RoundedRectangle(cornerRadius: Layout.smallRadius / 2)
                            .fill(fill(segment.style))
                            .overlay(RoundedRectangle(cornerRadius: Layout.smallRadius / 2).strokeBorder(Palette.borderStrong, lineWidth: segment.style == .free ? Stroke.hairline : 0))
                            .frame(width: Layout.iconSmall, height: Layout.iconSmall)
                        VStack(alignment: .leading, spacing: Space.xxs) {
                            Text(segment.label).font(TypeStyle.sectionTitle)
                            Text(format(segment.bytes)).font(TypeStyle.secondary).foregroundStyle(Palette.muted).monospacedDigit()
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityElement(children: .combine)
                }
            }
        }
    }
    private func fill(_ style: Style) -> Color {
        switch style {
        case .ready: Palette.accent
        case .found: Palette.accentSymbol.opacity(0.55)
        case .used: Palette.borderStrong
        case .purgeable: Palette.track
        case .free: Palette.surface
        }
    }
}

public struct ComponentGallery: View {
    @State private var checked = false
    @State private var size = "Any size"
    @State private var place = "Whole Mac"
    @State private var query = ""
    public init() {}
    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xl) {
                ToolHeader("Component Gallery", subtitle: "The shared vocabulary used by every module. Fixture data only.", symbol: "paintpalette") {
                    ActionButton("Scan", kind: .primary) {}
                }
                Panel {
                    VStack(alignment: .leading, spacing: Space.md) {
                        HStack(spacing: Space.sm) {
                            ChipLabel(symbol: "laptopcomputer", label: "Scanning", value: "Whole Mac")
                            SummaryLine([.init("128", "items"), .init("12.4 GB", "on disk"), .init("9.1 GB", "to review", emphasized: true)])
                        }
                        HStack(spacing: Space.sm) {
                            SearchField("Search by name or path", text: $query).frame(width: 240)
                            ChoiceChip("Size", selection: $size, neutral: "Any size", options: ["Any size", "Over 1 GB"].map { ChoiceOption($0, $0) })
                            ChoiceChip("Modified", selection: .constant("Any time"), neutral: "Any time", options: [ChoiceOption("Any time", "Any time")])
                            PillSegments(selection: $place, options: ["Whole Mac", "Chosen folders"].map { ChoiceOption($0, $0) })
                        }
                        HStack(spacing: Space.md) {
                            Toggle("Selected", isOn: $checked).labelsHidden().toggleStyle(SoftCheckboxStyle())
                            Toggle("Unselected", isOn: .constant(false)).labelsHidden().toggleStyle(SoftCheckboxStyle())
                            Pill("Rebuild required"); Pill("Review", tone: Palette.warning); Pill("Duplicate", tone: Palette.accentSymbol)
                            TextAction("Show items") {}
                        }
                    }
                }
                HStack(alignment: .top, spacing: Space.lg) {
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        BrandMark().padding(.bottom, Space.sm)
                        SidebarItem("Overview", symbol: "square.grid.2x2", selected: true) {}
                        SidebarHeading("Developer")
                        SidebarItem("Dependencies", symbol: "shippingbox", selected: false) {}
                        SidebarItem("Build Artifacts", symbol: "hammer", selected: false) {}
                    }
                    .padding(Space.sm).frame(width: Layout.sidebar).background(Palette.sidebar, in: RoundedRectangle(cornerRadius: Layout.panelRadius))
                    Callout("2 scan warnings · coverage may be incomplete", symbol: "exclamationmark.triangle.fill", tone: Palette.warning) {
                        Text("Cannot read ~/Library/Mail: permission denied").font(TypeStyle.caption).foregroundStyle(Palette.muted)
                    }
                }
                Panel {
                    DiskBar([
                        .init("Ready to free", bytes: 42_000_000_000, style: .ready),
                        .init("Other found", bytes: 61_000_000_000, style: .found),
                        .init("Purgeable", bytes: 12_000_000_000, style: .purgeable),
                        .init("Other used", bytes: 620_000_000_000, style: .used),
                        .init("Free", bytes: 260_000_000_000, style: .free),
                    ]) { ByteCountFormatter.string(fromByteCount: Int64($0), countStyle: .file) }
                }
                HStack(spacing: Space.xl) {
                    ScanOrb("Scan", subtitle: "Whole Mac", phase: .idle) {}
                    ScanOrb("Scan", phase: .scanning(progress: 0.42)) {}
                    ScanOrb("Scan again", phase: .idle, diameter: Layout.scanOrbCompact) {}
                }
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
                        FindingRow(title: "frontend", subtitle: "~/Projects/frontend/node_modules", value: "1.8 GB", badge: "Rebuild required", icon: .brand("pnpm"), active: false, eligible: true, checked: $checked) {}
                        FindingRow(title: "node", subtitle: "~/Projects/frontend", value: "53% CPU · 240 MB", badge: "Review", symbol: "cpu", active: true, eligible: true, checked: $checked) {}
                        FindingRow(title: "Main worktree", subtitle: "~/Projects/frontend", value: "2.1 GB", badge: "Protected", icon: .brand("git"), active: false, eligible: false, checked: .constant(false)) {}
                    }
                }
                Panel {
                    HStack(spacing: Space.md) {
                        ForEach(["nextdotjs", "react", "flutter", "rust", "python", "gradle", "swift", "xcode", "go", "dotnet", "unity", "pnpm"], id: \.self) { slug in
                            IconTile(icon: .brand(slug)).help(BrandCatalog.name(slug) ?? slug)
                        }
                    }
                }
                Panel { VStack(alignment: .leading, spacing: Space.lg) { KeyValueRow("State", "Incomplete scan · access denied"); KeyValueRow("Next action", "Choose a readable folder or grant access in System Settings.") } }
                EmptyState("No results", message: "Choose roots, then scan. A partial scan does not prove there is no clutter.")
            }.padding(Space.xl)
        }.background(Palette.canvas)
    }
}
