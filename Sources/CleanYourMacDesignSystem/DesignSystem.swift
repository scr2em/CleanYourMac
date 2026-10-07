import SwiftUI

public enum Space {
    public static let xxs: CGFloat = 2, xs: CGFloat = 4, sm: CGFloat = 8, md: CGFloat = 12, lg: CGFloat = 16, xl: CGFloat = 24, xxl: CGFloat = 32
}
public enum Layout {
    public static let sidebar: CGFloat = 220, inspector: CGFloat = 320, inspectorMin: CGFloat = 240, inspectorMax: CGFloat = 420
    public static let contentMin: CGFloat = 400, windowWidth: CGFloat = 1120, windowHeight: CGFloat = 760, windowMinWidth: CGFloat = 880, windowMinHeight: CGFloat = 560
    public static let reviewWidth: CGFloat = 640, reviewHeight: CGFloat = 540, rowMinimum: CGFloat = 52, panelRadius: CGFloat = 10
}
public enum TypeStyle {
    public static let pageTitle = Font.title2.weight(.semibold)
    public static let sectionTitle = Font.headline
    public static let body = Font.body
    public static let secondary = Font.callout
    public static let caption = Font.caption
    public static let metric = Font.title.weight(.semibold).monospacedDigit()
    public static let code = Font.system(.caption, design: .monospaced)
}
public enum Palette {
    public static let canvas = Color(nsColor: .windowBackgroundColor)
    public static let surface = Color(nsColor: .controlBackgroundColor)
    public static let elevated = Color(nsColor: .textBackgroundColor)
    public static let accent = Color.accentColor
    public static let warning = Color.orange
    public static let destructive = Color.red
    public static let success = Color.green
    public static let selection = Color.accentColor.opacity(0.12)
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
        Group {
            if kind == .secondary { button.buttonStyle(.bordered) }
            else { button.buttonStyle(.borderedProminent).tint(kind == .destructive ? Palette.destructive : Palette.accent) }
        }.disabled(disabled)
    }
    private var button: some View { Button(title, role: kind == .destructive ? .destructive : nil, action: action) }
}

public struct Panel<Content: View>: View {
    private let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public var body: some View {
        content.padding(Space.lg).frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: Layout.panelRadius))
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
                    Image(systemName: symbol).foregroundStyle(Palette.accent)
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
                Capsule().fill(Palette.selection)
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
