import SwiftUI

/// The overview's scan control: a glowing orb in a slowly orbiting field of particles. It is
/// the Scan button at rest, speeds up and shows a progress ring while scanning, and shrinks
/// to a compact header button once results are in. Reduce Motion freezes the animation.
public struct ScanOrb: View {
    public enum Phase: Equatable {
        case idle
        /// `progress` is the completed share, or `nil` when unknown.
        case scanning(progress: Double?)
    }
    private let phase: Phase
    private let title: String
    private let subtitle: String
    private let diameter: CGFloat
    private let action: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.isEnabled) private var isEnabled

    public init(_ title: String, subtitle: String = "", phase: Phase, diameter: CGFloat = Layout.scanOrb, action: @escaping () -> Void) {
        self.title = title; self.subtitle = subtitle; self.phase = phase; self.diameter = diameter; self.action = action
    }

    private var scanning: Bool { if case .scanning = phase { true } else { false } }
    private var compact: Bool { diameter < Layout.scanOrb / 2 }

    public var body: some View {
        Button(action: action) {
            TimelineView(.animation(minimumInterval: 1.0 / 60, paused: reduceMotion)) { timeline in
                let time = reduceMotion ? 0 : timeline.date.timeIntervalSinceReferenceDate
                orb(at: time)
            }
            .frame(width: diameter * (compact ? 1.2 : 2), height: diameter * (compact ? 1.2 : 1.5))
            .contentShape(Circle().scale(compact ? 1 : 0.6))
        }
        .buttonStyle(.plain)
        .disabled(scanning)
        .accessibilityLabel(scanning ? "Scanning" : title)
        .accessibilityValue(progressValue.map { "\(Int($0 * 100)) percent" } ?? "")
    }

    private var progressValue: Double? { if case .scanning(let progress) = phase { progress } else { nil } }

    @ViewBuilder private func orb(at time: Double) -> some View {
        let speed = scanning ? 1.0 : 0.18
        ZStack {
            // Nebula glow behind everything.
            Circle()
                .fill(RadialGradient(colors: [Palette.accentSymbol.opacity(scanning ? 0.45 : 0.3), Palette.accentSymbol.opacity(0)], center: .center, startRadius: 0, endRadius: diameter * 0.8))
                .frame(width: diameter * 1.6, height: diameter * 1.6)
                .scaleEffect(1 + 0.04 * sin(time * (scanning ? 2.4 : 0.8)))
            if !compact { particles(at: time, speed: speed) }
            // Two counter-rotating orbit rings.
            ring(width: diameter * 1.18, angle: time * 0.6 * speed * 3, tilt: 62)
            ring(width: diameter * 1.34, angle: -time * 0.4 * speed * 3, tilt: 70)
            // The core.
            Circle()
                .fill(RadialGradient(colors: [Palette.accentSymbol, Palette.accent], center: UnitPoint(x: 0.35, y: 0.3), startRadius: 1, endRadius: diameter * 0.75))
                .overlay(Circle().fill(AngularGradient(colors: [Palette.onAccent.opacity(0), Palette.onAccent.opacity(0.18), Palette.onAccent.opacity(0)], center: .center)).rotationEffect(.radians(time * speed * 2)))
                .frame(width: diameter, height: diameter)
                .shadow(color: Palette.accent.opacity(0.45), radius: diameter * 0.15)
                .scaleEffect(1 + 0.015 * sin(time * 1.6))
            progressRing(at: time)
            VStack(spacing: Space.xxs) {
                Text(label).font(compact ? TypeStyle.captionEmphasis : TypeStyle.title).monospacedDigit()
                if !compact && !subtitle.isEmpty {
                    Text(subtitle).font(TypeStyle.caption).opacity(0.8).multilineTextAlignment(.center).lineLimit(2)
                }
            }
            .foregroundStyle(Palette.onAccent)
            .padding(.horizontal, Space.lg)
            .frame(width: diameter)
        }
        .opacity(isEnabled || scanning ? 1 : Opacity.disabled)
    }

    private var label: String {
        if case .scanning(let progress) = phase {
            return progress.map { "\(Int(($0 * 100).rounded()))%" } ?? "Scanning"
        }
        return title
    }

    private func ring(width: CGFloat, angle: Double, tilt: Double) -> some View {
        Circle()
            .strokeBorder(AngularGradient(colors: [Palette.accentSymbol.opacity(0.7), Palette.accentSymbol.opacity(0.05), Palette.accentSymbol.opacity(0.7)], center: .center), lineWidth: Stroke.hairline)
            .frame(width: width, height: width)
            .rotationEffect(.radians(angle))
            .rotation3DEffect(.degrees(tilt), axis: (x: 1, y: 0, z: 0))
    }

    @ViewBuilder private func progressRing(at time: Double) -> some View {
        if case .scanning(let progress) = phase {
            let ring = Circle()
            Group {
                if let progress {
                    ring.trim(from: 0, to: max(0.02, progress)).rotation(.degrees(-90))
                        .stroke(Palette.onAccent, style: StrokeStyle(lineWidth: Stroke.focus * 2, lineCap: .round))
                } else {
                    ring.trim(from: 0, to: 0.22).rotation(.radians(time * 3))
                        .stroke(Palette.onAccent, style: StrokeStyle(lineWidth: Stroke.focus * 2, lineCap: .round))
                }
            }
            .frame(width: diameter * 0.88, height: diameter * 0.88)
            .animation(.easeOut(duration: 0.4), value: progress)
        }
    }

    /// Stars on tilted elliptical orbits, deterministic per index so they never jump.
    private func particles(at time: Double, speed: Double) -> some View {
        Canvas { context, size in
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            func fract(_ x: Double) -> Double { x - x.rounded(.down) }
            for index in 0..<90 {
                let seed = Double(index) * 12.9898
                let radius = Double(diameter) * (0.62 + 0.38 * fract(seed * 0.43))
                let angle = seed + time * speed * (0.25 + fract(seed * 0.37))
                let point = CGPoint(x: center.x + cos(angle) * radius, y: center.y + sin(angle) * radius * 0.42)
                let dot = 1 + 2.2 * fract(seed * 0.73)
                let twinkle = 0.35 + 0.55 * (0.5 + 0.5 * sin(time * 2 + seed))
                context.fill(Path(ellipseIn: CGRect(x: point.x - dot / 2, y: point.y - dot / 2, width: dot, height: dot)), with: .color(Palette.accentSymbol.opacity(twinkle)))
            }
        }
        .allowsHitTesting(false)
    }
}
