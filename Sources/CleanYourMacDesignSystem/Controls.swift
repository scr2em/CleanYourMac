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
    private let title: String, subtitle: String, symbol: String
    private let trailing: Trailing
    public init(_ title: String, subtitle: String, symbol: String, @ViewBuilder trailing: () -> Trailing) {
        self.title = title; self.subtitle = subtitle; self.symbol = symbol; self.trailing = trailing()
    }
    public var body: some View {
        HStack(alignment: .center, spacing: Space.md) {
            Image(systemName: symbol).font(TypeStyle.title).foregroundStyle(Palette.accentSymbol)
                .frame(width: Layout.toolIcon, height: Layout.toolIcon)
                .background(Palette.selection, in: RoundedRectangle(cornerRadius: Layout.controlRadius))
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
