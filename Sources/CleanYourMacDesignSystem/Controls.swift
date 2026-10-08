import SwiftUI

// The app's own controls for toolbars: pill-shaped chips that open a compact list, a search
// field and a page header with the tool's icon. They replace stock pickers and text fields so
// toolbars read as one piece and follow the theme.

/// One option of a `ChoiceChip`.
public struct ChoiceOption<Value: Hashable>: Identifiable {
    public let value: Value, title: String
    public var id: Value { value }
    public init(_ value: Value, _ title: String) { self.value = value; self.title = title }
}

/// A pill showing a setting and its current value ("Size · Any"). Clicking opens the options.
public struct ChoiceChip<Value: Hashable>: View {
    private let label: String?, symbol: String?
    private let options: [ChoiceOption<Value>]
    @Binding private var selection: Value
    private let neutral: Value?
    @State private var open = false
    /// `neutral` is the value that means "no filter"; any other value highlights the chip.
    public init(_ label: String? = nil, symbol: String? = nil, selection: Binding<Value>, neutral: Value? = nil, options: [ChoiceOption<Value>]) {
        self.label = label; self.symbol = symbol; self._selection = selection; self.neutral = neutral; self.options = options
    }
    private var active: Bool { neutral.map { $0 != selection } ?? false }
    public var body: some View {
        Button { open.toggle() } label: {
            ChipLabel(symbol: symbol, label: label, value: options.first { $0.value == selection }?.title ?? "", active: active, open: open)
        }
        .buttonStyle(.plain)
        .popover(isPresented: $open, arrowEdge: .bottom) {
            OptionList(options: options.map { ($0.id, $0.title) }, selected: selection) { value in
                selection = value; open = false
            }
        }
        .accessibilityLabel([label, options.first { $0.value == selection }?.title].compactMap { $0 }.joined(separator: ", "))
    }
}

/// The visual of a chip, shared with chips that open custom content.
public struct ChipLabel: View {
    let symbol: String?, label: String?, value: String, active: Bool, open: Bool
    public init(symbol: String? = nil, label: String? = nil, value: String, active: Bool = false, open: Bool = false) {
        self.symbol = symbol; self.label = label; self.value = value; self.active = active; self.open = open
    }
    public var body: some View {
        HStack(spacing: Space.xs) {
            if let symbol { Image(systemName: symbol).font(TypeStyle.caption).foregroundStyle(active ? Palette.accentText : Palette.muted) }
            if let label { Text(label).foregroundStyle(Palette.muted) }
            Text(value).foregroundStyle(active ? Palette.accentText : Palette.ink).lineLimit(1)
            Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold)).foregroundStyle(Palette.muted)
                .rotationEffect(.degrees(open ? 180 : 0))
        }
        .font(TypeStyle.label)
        .padding(.horizontal, Space.md).frame(height: Layout.chipHeight)
        .background(active ? Palette.selection : Palette.surface, in: Capsule())
        .overlay(Capsule().strokeBorder(active ? Palette.accent.opacity(0.5) : Palette.border, lineWidth: Stroke.hairline))
        .contentShape(Capsule())
        .animation(Motion.quick, value: open)
    }
}

/// A short list of choices with a check on the current one.
struct OptionList<ID: Hashable>: View {
    let options: [(ID, String)], selected: ID
    let choose: (ID) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            ForEach(options, id: \.0) { option in
                OptionRow(title: option.1, checked: option.0 == selected) { choose(option.0) }
            }
        }
        .padding(Space.xs).frame(minWidth: 180)
        .background(Palette.elevated)
    }
}

/// One row of a chip's list.
public struct OptionRow: View {
    let title: String, symbol: String?, checked: Bool
    let action: () -> Void
    @State private var hovered = false
    public init(title: String, symbol: String? = nil, checked: Bool = false, action: @escaping () -> Void) {
        self.title = title; self.symbol = symbol; self.checked = checked; self.action = action
    }
    public var body: some View {
        Button(action: action) {
            HStack(spacing: Space.sm) {
                Image(systemName: symbol ?? "checkmark").font(TypeStyle.caption.weight(.semibold))
                    .foregroundStyle(Palette.accentSymbol).opacity(symbol != nil || checked ? 1 : 0).frame(width: Layout.iconSmall)
                Text(title).font(TypeStyle.secondary).foregroundStyle(Palette.ink).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: Space.lg)
            }
            .padding(.horizontal, Space.sm).frame(height: Layout.chipHeight)
            .background(hovered ? Palette.selection : .clear, in: RoundedRectangle(cornerRadius: Layout.smallRadius + 2))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
    }
}

/// The app's search field: a pill with a magnifier and a clear button.
public struct SearchField: View {
    private let prompt: String
    @Binding private var text: String
    @FocusState private var focused: Bool
    public init(_ prompt: String, text: Binding<String>) { self.prompt = prompt; self._text = text }
    public var body: some View {
        HStack(spacing: Space.sm) {
            Image(systemName: "magnifyingglass").font(TypeStyle.label).foregroundStyle(focused ? Palette.accentSymbol : Palette.muted)
            TextField(prompt, text: $text).textFieldStyle(.plain).font(TypeStyle.secondary).focused($focused)
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.muted) }
                    .buttonStyle(.plain).accessibilityLabel("Clear search")
            }
        }
        .padding(.horizontal, Space.md).frame(height: Layout.chipHeight)
        .background(Palette.surface, in: Capsule())
        .overlay(Capsule().strokeBorder(focused ? Palette.accent : Palette.border, lineWidth: focused ? Stroke.focus : Stroke.hairline))
        .animation(Motion.quick, value: focused)
    }
}

/// A tool's title, with its icon in a tinted tile and a one-line description.
public struct ToolHeader<Trailing: View>: View {
    private let title: String, subtitle: String, symbol: String, tone: IconTone?
    private let trailing: Trailing
    public init(_ title: String, subtitle: String, symbol: String, tone: IconTone? = nil, @ViewBuilder trailing: () -> Trailing) {
        self.title = title; self.subtitle = subtitle; self.symbol = symbol; self.tone = tone; self.trailing = trailing()
    }
    @ViewBuilder private var tile: some View {
        if let tone {
            ToolIcon(symbol, tone: tone, size: .header)
        } else {
            Image(systemName: symbol).font(TypeStyle.title).foregroundStyle(Palette.accentSymbol)
                .frame(width: Layout.toolIcon, height: Layout.toolIcon)
                .background(Palette.selection, in: RoundedRectangle(cornerRadius: Layout.controlRadius))
        }
    }
    public var body: some View {
        HStack(alignment: .center, spacing: Space.md) {
            tile
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).font(TypeStyle.pageTitle).foregroundStyle(Palette.ink)
                Text(subtitle).font(TypeStyle.secondary).foregroundStyle(Palette.muted).lineLimit(1).truncationMode(.tail).help(subtitle)
            }
            Spacer(minLength: Space.lg)
            trailing
        }
    }
}

/// Figures in one line, separated by dots: "12 items · 3.4 GB on disk · 2.1 GB to review".
public struct SummaryLine: View {
    public struct Part: Identifiable {
        public let id = UUID()
        let value: String, label: String, emphasized: Bool
        public init(_ value: String, _ label: String, emphasized: Bool = false) { self.value = value; self.label = label; self.emphasized = emphasized }
    }
    private let parts: [Part]
    public init(_ parts: [Part]) { self.parts = parts }
    public var body: some View {
        HStack(spacing: Space.sm) {
            ForEach(Array(parts.enumerated()), id: \.element.id) { index, part in
                if index > 0 { Circle().fill(Palette.border).frame(width: 3, height: 3) }
                (Text(part.value).font(TypeStyle.label.monospacedDigit()).foregroundColor(part.emphasized ? Palette.accentText : Palette.ink)
                 + Text(" " + part.label).font(TypeStyle.body).foregroundColor(Palette.muted))
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The app's checkbox: a rounded square that fills with the accent and bounces when ticked.
public struct SoftCheckboxStyle: ToggleStyle {
    @Environment(\.isEnabled) private var enabled
    public init() {}
    public func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(configuration.isOn ? Palette.accent : Palette.surface)
                .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .strokeBorder(configuration.isOn ? .clear : Palette.borderStrong, lineWidth: 1.5))
                .overlay(Image(systemName: "checkmark").font(.system(size: 10, weight: .heavy, design: .rounded))
                    .foregroundStyle(Palette.onAccent).scaleEffect(configuration.isOn ? 1 : 0.3).opacity(configuration.isOn ? 1 : 0))
                .frame(width: Layout.checkbox, height: Layout.checkbox)
                .animation(Motion.press, value: configuration.isOn)
                .opacity(enabled ? 1 : Opacity.disabled)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // VoiceOver hears an ordinary checkbox.
        .accessibilityRepresentation { Toggle(isOn: configuration.$isOn) { configuration.label } }
    }
}

/// A row's icon on a soft tinted tile, so symbols, logos and file icons sit alike. With a
/// tone, a symbol takes its tool's color on a faint wash of it; logos and file icons keep
/// their own colors on the neutral tile.
public struct IconTile: View {
    let icon: RowIcon, tone: IconTone?
    public init(icon: RowIcon, tone: IconTone? = nil) { self.icon = icon; self.tone = tone }
    private var symbolTone: IconTone? {
        switch icon {
        case .symbol: return tone
        default: return nil
        }
    }
    public var body: some View {
        RowIconView(icon: icon, tint: symbolTone?.color)
            .frame(width: Layout.rowTile, height: Layout.rowTile)
            .background(symbolTone.map { $0.color.opacity(0.14) } ?? Palette.track,
                        in: RoundedRectangle(cornerRadius: Layout.tileRadius, style: .continuous))
    }
}

/// A small rounded label for a row's state, such as "Rebuild required".
public struct Pill: View {
    private let text: String, tone: Color?
    public init(_ text: String, tone: Color? = nil) { self.text = text; self.tone = tone }
    public var body: some View {
        Text(text).font(TypeStyle.captionEmphasis).lineLimit(1)
            .foregroundStyle(tone ?? Palette.muted)
            .padding(.horizontal, Space.sm).padding(.vertical, Space.xxs)
            .background((tone ?? Palette.muted).opacity(0.12), in: Capsule())
    }
}

/// A tool's symbol in white on a colored tile, as in the sidebar and tool headers.
public struct ToolIcon: View {
    public enum Size: Sendable { case small, row, header }
    private let symbol: String, tone: IconTone, size: Size
    public init(_ symbol: String, tone: IconTone, size: Size = .row) {
        self.symbol = symbol; self.tone = tone; self.size = size
    }
    private var side: CGFloat {
        switch size {
        case .small: return Layout.sidebarTile
        case .row: return Layout.rowTile
        case .header: return Layout.toolIcon
        }
    }
    private var font: Font {
        switch size {
        case .small: return .system(size: 12, weight: .semibold, design: .rounded)
        case .row: return TypeStyle.headline
        case .header: return TypeStyle.title
        }
    }
    private var radius: CGFloat {
        switch size {
        case .small: return Layout.smallRadius
        case .row: return Layout.tileRadius
        case .header: return Layout.controlRadius
        }
    }
    public var body: some View {
        Image(systemName: symbol).font(font).foregroundStyle(.white)
            .frame(width: side, height: side)
            .background(tone.color, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// One destination in the sidebar: an icon tile and a name on a pill that fills when selected.
/// With a tone the tile keeps its color when selected, and the pill marks the selection.
public struct SidebarItem: View {
    private let title: String, symbol: String, selected: Bool, tone: IconTone?
    private let action: () -> Void
    @State private var hovered = false
    public init(_ title: String, symbol: String, selected: Bool, tone: IconTone? = nil, action: @escaping () -> Void) {
        self.title = title; self.symbol = symbol; self.selected = selected; self.tone = tone; self.action = action
    }
    @ViewBuilder private var tile: some View {
        if let tone {
            ToolIcon(symbol, tone: tone, size: .small)
        } else {
            Image(systemName: symbol).font(.system(size: 12, weight: .semibold, design: .rounded))
                .foregroundStyle(selected ? Palette.onAccent : Palette.accentSymbol)
                .frame(width: Layout.sidebarTile, height: Layout.sidebarTile)
                .background(selected ? Palette.accent : Palette.track, in: RoundedRectangle(cornerRadius: Layout.smallRadius, style: .continuous))
        }
    }
    /// The row's pill: a soft wash of the tool's color when it has one.
    private var highlight: Color {
        if let tone { return tone.color.opacity(selected ? 0.16 : 0.08) }
        return selected ? Palette.sidebarSelection : Palette.sidebarSelection.opacity(0.5)
    }
    public var body: some View {
        Button(action: action) {
            HStack(spacing: Space.sm + Space.xxs) {
                tile
                Text(title).font(TypeStyle.label).foregroundStyle(Palette.ink).lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, Space.xs + Space.xxs).frame(height: Layout.sidebarRow)
            .background(selected || hovered ? highlight : .clear,
                        in: RoundedRectangle(cornerRadius: Layout.rowRadius, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .animation(Motion.quick, value: hovered)
        .animation(Motion.press, value: selected)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
    }
}

/// A sidebar group's heading.
public struct SidebarHeading: View {
    private let title: String
    public init(_ title: String) { self.title = title }
    public var body: some View {
        Text(title).font(TypeStyle.captionEmphasis).foregroundStyle(Palette.muted)
            .padding(.horizontal, Space.sm).padding(.top, Space.md).padding(.bottom, Space.xxs)
            .accessibilityAddTraits(.isHeader)
    }
}

/// The app's mark and name, at the top of the sidebar.
public struct BrandMark: View {
    public init() {}
    public var body: some View {
        HStack(spacing: Space.sm) {
            Image(systemName: "leaf.fill").font(.system(size: 14, weight: .bold, design: .rounded))
                .foregroundStyle(Palette.onAccent)
                .frame(width: 30, height: 30)
                .background(LinearGradient(colors: [Palette.accentSymbol, Palette.accent], startPoint: .topLeading, endPoint: .bottomTrailing),
                            in: RoundedRectangle(cornerRadius: Layout.tileRadius, style: .continuous))
                .softShadow(0.5)
            Text("CleanYourMac").font(.system(size: 15, weight: .bold, design: .rounded)).foregroundStyle(Palette.ink)
        }
        .accessibilityElement(children: .combine)
    }
}

/// A pill-shaped segmented control whose selection slides between options.
public struct PillSegments<Value: Hashable>: View {
    private let options: [ChoiceOption<Value>]
    @Binding private var selection: Value
    @Namespace private var thumb
    public init(selection: Binding<Value>, options: [ChoiceOption<Value>]) { self._selection = selection; self.options = options }
    public var body: some View {
        HStack(spacing: Space.xxs) {
            ForEach(options) { option in
                let chosen = option.value == selection
                Button { selection = option.value } label: {
                    Text(option.title).font(TypeStyle.label)
                        .foregroundStyle(chosen ? Palette.ink : Palette.muted)
                        .padding(.horizontal, Space.md).frame(height: Layout.chipHeight - Space.xs)
                        .background {
                            if chosen {
                                Capsule().fill(Palette.surface).softShadow(0.4).matchedGeometryEffect(id: "thumb", in: thumb)
                            }
                        }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(chosen ? [.isButton, .isSelected] : .isButton)
            }
        }
        .padding(Space.xxs)
        .background(Palette.track, in: Capsule())
        .animation(Motion.press, value: selection)
    }
}

/// A text-only button in the accent colour, for secondary links inside cards.
public struct TextAction: View {
    private let title: String, action: () -> Void
    public init(_ title: String, action: @escaping () -> Void) { self.title = title; self.action = action }
    public var body: some View {
        Button(action: action) { Text(title).font(TypeStyle.captionEmphasis).foregroundStyle(Palette.accentText) }
            .buttonStyle(.plain)
    }
}

/// A soft card that expands to show details, used for scan warnings. The details scroll
/// inside a bounded height, so a long list never pushes the page out of reach, and the
/// optional action stays in the header where it is always visible.
public struct Callout<Content: View>: View {
    private let title: String, symbol: String, tone: Color
    private let action: (title: String, run: () -> Void)?
    private let content: Content
    @State private var open = false
    public init(_ title: String, symbol: String, tone: Color, action: (title: String, run: () -> Void)? = nil, @ViewBuilder content: () -> Content) {
        self.title = title; self.symbol = symbol; self.tone = tone; self.action = action; self.content = content()
    }
    public var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            HStack(spacing: Space.sm) {
                Button { open.toggle() } label: {
                    HStack(spacing: Space.sm) {
                        Image(systemName: symbol).font(TypeStyle.label).foregroundStyle(tone)
                        Text(title).font(TypeStyle.label).foregroundStyle(Palette.ink).lineLimit(2).multilineTextAlignment(.leading)
                        Image(systemName: "chevron.down").font(.system(size: 9, weight: .bold)).foregroundStyle(Palette.muted)
                            .rotationEffect(.degrees(open ? 180 : 0))
                        Spacer(minLength: Space.sm)
                    }.contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(open ? "Hide details: \(title)" : "Show details: \(title)")
                if let action { TextAction(action.title, action: action.run) }
            }
            if open {
                ScrollView {
                    content.frame(maxWidth: .infinity, alignment: .leading).padding(.trailing, Space.sm)
                }
                .frame(maxHeight: Layout.calloutMaxHeight)
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .padding(.horizontal, Space.md).padding(.vertical, Space.sm + Space.xxs)
        .background(tone.opacity(0.08), in: RoundedRectangle(cornerRadius: Layout.controlRadius, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Layout.controlRadius, style: .continuous).strokeBorder(tone.opacity(0.25), lineWidth: Stroke.hairline))
        .animation(Motion.press, value: open)
    }
}
