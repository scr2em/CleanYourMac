import AppKit
import SwiftUI

public enum Space {
    public static let xxs: CGFloat = 2, xs: CGFloat = 4, sm: CGFloat = 8, md: CGFloat = 12, lg: CGFloat = 16, xl: CGFloat = 24, xxl: CGFloat = 32
}
public enum Layout {
    public static let sidebar: CGFloat = 220, inspector: CGFloat = 320, inspectorMin: CGFloat = 240, inspectorMax: CGFloat = 420
    public static let contentMin: CGFloat = 400, windowWidth: CGFloat = 1120, windowHeight: CGFloat = 760, windowMinWidth: CGFloat = 880, windowMinHeight: CGFloat = 560
    public static let reviewWidth: CGFloat = 640, reviewHeight: CGFloat = 540, rowMinimum: CGFloat = 52, panelRadius: CGFloat = 16, controlRadius: CGFloat = 10
}
public enum TypeStyle {
    public static let pageTitle = Font.system(size: 28, weight: .semibold, design: .serif)
    public static let sectionTitle = Font.system(size: 17, weight: .semibold, design: .serif)
    public static let body = Font.body
    public static let secondary = Font.callout
    public static let caption = Font.caption
    public static let metric = Font.system(size: 32, weight: .semibold, design: .serif).monospacedDigit()
    public static let code = Font.system(.caption, design: .monospaced)
}
/// Comfy palette: warm off-white canvas, linen and sand neutrals and a dusty-rose accent,
/// with a warm espresso dark mode. Values mirror docs/brand/tokens.json.
public enum Palette {
    public static let canvas = dynamic(0xF9F5F2, 0x1E1A17)
    public static let sidebar = dynamic(0xEDE3D5, 0x26211D)
    public static let surface = dynamic(0xFDFBF9, 0x2B2622)
    public static let elevated = dynamic(0xFFFFFF, 0x332D28)
    public static let ink = dynamic(0x2B2621, 0xF1EBE1)
    public static let muted = dynamic(0x6B6157, 0xB3A99B)
    public static let border = dynamic(0xD6C4B2, 0x4A4038)
    public static let track = dynamic(0xEDE3D5, 0x3A332D)
    /// Fills: primary buttons, bars and highlights. Pair with `onAccent` text.
    public static let accent = dynamic(0xD0A097, 0xD9A79E)
    public static let onAccent = dynamic(0x2B2621, 0x2A1A16)
    /// Accent-colored text and symbols on cream surfaces.
    public static let accentText = dynamic(0x9A5F55, 0xE2B3AA)
    public static let leaf = dynamic(0x5EBEA5, 0x76CFB4)
    public static let warning = dynamic(0x7A520F, 0xEDC783)
    public static let destructive = dynamic(0xA12D2A, 0xF0857A)
    public static let success = dynamic(0x3B7A5E, 0x86C9A8)
    public static let selection = dynamic(0xEFE0D9, 0x4A3530)

    private static func dynamic(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let hex = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
            return NSColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
        })
    }
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
            .overlay(RoundedRectangle(cornerRadius: Layout.controlRadius).strokeBorder(kind == .secondary ? Palette.border : .clear))
            .opacity(enabled ? (configuration.isPressed ? 0.8 : 1) : 0.45)
            .contentShape(RoundedRectangle(cornerRadius: Layout.controlRadius))
    }
    private var fill: Color {
        switch kind { case .primary: Palette.accent; case .secondary: Palette.surface; case .destructive: Palette.destructive }
    }
    private var foreground: Color {
        switch kind { case .primary: Palette.onAccent; case .secondary: Palette.ink; case .destructive: Palette.canvas }
    }
}

public struct Panel<Content: View>: View {
    private let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public var body: some View {
        content.padding(Space.lg).frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Layout.panelRadius))
            .overlay(RoundedRectangle(cornerRadius: Layout.panelRadius).strokeBorder(Palette.border.opacity(0.6)))
            .shadow(color: Palette.ink.opacity(0.06), radius: Space.sm, y: Space.xxs)
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

public struct ResultRow: View {
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
                    Image(systemName: symbol).foregroundStyle(Palette.accentText)
                    VStack(alignment: .leading, spacing: Space.xs) {
                        Text(title).font(TypeStyle.body).lineLimit(1)
                        Text(subtitle).font(TypeStyle.caption).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: Space.xs) {
                        Text(value).font(TypeStyle.secondary).monospacedDigit()
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
                Panel { HStack { StatusBadge("Complete"); StatusBadge("Partial", warning: true); StatusBadge("Permanent", warning: true) } }
                Panel { StorageBar("Developer data", value: "12.4 GB", fraction: 0.6) }
                Panel {
                    VStack(spacing: Space.xs) {
                        ResultRow(title: "frontend", subtitle: "~/Projects/frontend/node_modules", value: "1.8 GB", badge: "Rebuild required", symbol: "shippingbox", active: false, eligible: true, checked: $checked) {}
                        ResultRow(title: "node", subtitle: "~/Projects/frontend", value: "53% CPU · 240 MB", badge: "Review", symbol: "cpu", active: true, eligible: true, checked: $checked) {}
                        ResultRow(title: "Main worktree", subtitle: "~/Projects/frontend", value: "2.1 GB", badge: "Protected", symbol: "arrow.triangle.branch", active: false, eligible: false, checked: .constant(false)) {}
                    }
                }
                Panel { VStack(alignment: .leading, spacing: Space.lg) { KeyValueRow("State", "Incomplete scan · access denied"); KeyValueRow("Next action", "Choose a readable folder or grant access in System Settings.") } }
                EmptyState("No results", message: "Choose roots, then scan. A partial scan does not prove there is no clutter.")
            }.padding(Space.xl)
        }.background(Palette.canvas)
    }
}
