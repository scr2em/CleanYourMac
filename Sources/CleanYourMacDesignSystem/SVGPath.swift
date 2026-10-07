import Foundation

/// Absolute drawing commands read from SVG path data. Arcs become cubic curves, and
/// shorthand curves are expanded, so a renderer needs only these five cases.
public enum SVGCommand: Equatable, Sendable {
    case move(Double, Double)
    case line(Double, Double)
    case cubic(Double, Double, Double, Double, Double, Double)
    case quad(Double, Double, Double, Double)
    case close
}

/// A single-colour SVG icon: its title, fill colour and every path, in its view box.
public struct SVGIcon: Sendable {
    public let title: String?
    /// `0xRRGGBB`, from the root element's `fill`.
    public let fill: UInt32?
    public let viewBox: (x: Double, y: Double, width: Double, height: Double)
    public let commands: [SVGCommand]

    /// Reads the `<svg>` element's `viewBox` and `fill`, the `<title>`, and every `<path d>`.
    public init?(svg: String) {
        guard let root = svg.range(of: "<svg"), let rootEnd = svg[root.upperBound...].firstIndex(of: ">") else { return nil }
        let element = String(svg[root.upperBound..<rootEnd])
        let box = SVGIcon.attribute("viewBox", in: element)?
            .split(whereSeparator: { $0 == " " || $0 == "," })
            .compactMap { Double($0) } ?? []
        viewBox = box.count == 4 ? (box[0], box[1], box[2], box[3]) : (0, 0, 24, 24)
        fill = SVGIcon.attribute("fill", in: element).flatMap { value in
            value.hasPrefix("#") && value.count == 7 ? UInt32(value.dropFirst(), radix: 16) : nil
        }
        if let start = svg.range(of: "<title>"), let end = svg.range(of: "</title>", range: start.upperBound..<svg.endIndex) {
            title = String(svg[start.upperBound..<end.lowerBound])
        } else {
            title = nil
        }
        var commands: [SVGCommand] = []
        var rest = svg[rootEnd...]
        while let tag = rest.range(of: "<path") {
            guard let end = rest[tag.upperBound...].firstIndex(of: ">") else { break }
            if let data = SVGIcon.attribute("d", in: String(rest[tag.upperBound..<end])) {
                commands += SVGPathParser.parse(data)
            }
            rest = rest[end...]
        }
        guard !commands.isEmpty else { return nil }
        self.commands = commands
    }

    private static func attribute(_ name: String, in element: String) -> String? {
        for quote in ["\"", "'"] {
            if let start = element.range(of: " \(name)=\(quote)"),
               let end = element[start.upperBound...].firstIndex(of: Character(quote)) {
                return String(element[start.upperBound..<end])
            }
        }
        return nil
    }
}

/// SVG path data (`M`, `L`, `H`, `V`, `C`, `S`, `Q`, `T`, `A`, `Z`, absolute or relative).
public enum SVGPathParser {
    public static func parse(_ data: String) -> [SVGCommand] {
        var scanner = Scanner(Array(data.utf8))
        var out: [SVGCommand] = []
        var x = 0.0, y = 0.0, startX = 0.0, startY = 0.0
        // The last cubic or quadratic control point, for reflecting shorthand curves.
        var lastCubic: (Double, Double)?
        var lastQuad: (Double, Double)?
        var command: UInt8?
        while true {
            scanner.skipSeparators()
            guard let byte = scanner.peek() else { break }
            if Scanner.isCommand(byte) {
                command = byte
                scanner.advance()
                if byte == UInt8(ascii: "Z") || byte == UInt8(ascii: "z") {
                    out.append(.close)
                    x = startX; y = startY
                    lastCubic = nil; lastQuad = nil
                    continue
                }
            }
            guard let current = command else { break }
            let relative = current >= UInt8(ascii: "a")
            let ox = relative ? x : 0, oy = relative ? y : 0
            var cubic: (Double, Double)?
            var quad: (Double, Double)?
            switch current | 0x20 { // lower-cased
            case UInt8(ascii: "m"):
                guard let a = scanner.numbers(2) else { return out }
                x = a[0] + ox; y = a[1] + oy
                startX = x; startY = y
                out.append(.move(x, y))
                // Further pairs after a move are lines.
                command = relative ? UInt8(ascii: "l") : UInt8(ascii: "L")
            case UInt8(ascii: "l"):
                guard let a = scanner.numbers(2) else { return out }
                x = a[0] + ox; y = a[1] + oy
                out.append(.line(x, y))
            case UInt8(ascii: "h"):
                guard let a = scanner.numbers(1) else { return out }
                x = a[0] + ox
                out.append(.line(x, y))
            case UInt8(ascii: "v"):
                guard let a = scanner.numbers(1) else { return out }
                y = a[0] + oy
                out.append(.line(x, y))
            case UInt8(ascii: "c"):
                guard let a = scanner.numbers(6) else { return out }
                let c2 = (a[2] + ox, a[3] + oy)
                x = a[4] + ox; y = a[5] + oy
                out.append(.cubic(a[0] + ox, a[1] + oy, c2.0, c2.1, x, y))
                cubic = c2
            case UInt8(ascii: "s"):
                guard let a = scanner.numbers(4) else { return out }
                let c1 = lastCubic.map { (2 * x - $0.0, 2 * y - $0.1) } ?? (x, y)
                let c2 = (a[0] + ox, a[1] + oy)
                x = a[2] + ox; y = a[3] + oy
                out.append(.cubic(c1.0, c1.1, c2.0, c2.1, x, y))
                cubic = c2
            case UInt8(ascii: "q"):
                guard let a = scanner.numbers(4) else { return out }
                let c = (a[0] + ox, a[1] + oy)
                x = a[2] + ox; y = a[3] + oy
                out.append(.quad(c.0, c.1, x, y))
                quad = c
            case UInt8(ascii: "t"):
                guard let a = scanner.numbers(2) else { return out }
                let c = lastQuad.map { (2 * x - $0.0, 2 * y - $0.1) } ?? (x, y)
                x = a[0] + ox; y = a[1] + oy
                out.append(.quad(c.0, c.1, x, y))
                quad = c
            case UInt8(ascii: "a"):
                guard let radii = scanner.numbers(3), let large = scanner.flag(), let sweep = scanner.flag(),
                      let end = scanner.numbers(2) else { return out }
                let nx = end[0] + ox, ny = end[1] + oy
                out += arc(from: (x, y), radii: (radii[0], radii[1]), rotation: radii[2], large: large, sweep: sweep, to: (nx, ny))
                x = nx; y = ny
            default:
                return out
            }
            lastCubic = cubic
            lastQuad = quad
        }
        return out
    }

    /// An endpoint-parameterised arc as cubic curves (SVG implementation notes, F.6).
    static func arc(from p1: (Double, Double), radii: (Double, Double), rotation: Double, large: Bool, sweep: Bool, to p2: (Double, Double)) -> [SVGCommand] {
        if p1 == p2 { return [] }
        var rx = abs(radii.0), ry = abs(radii.1)
        if rx == 0 || ry == 0 { return [.line(p2.0, p2.1)] }
        let phi = rotation.truncatingRemainder(dividingBy: 360) * .pi / 180
        let cosPhi = cos(phi), sinPhi = sin(phi)
        let dx = (p1.0 - p2.0) / 2, dy = (p1.1 - p2.1) / 2
        let x1 = cosPhi * dx + sinPhi * dy, y1 = -sinPhi * dx + cosPhi * dy
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
        if lambda > 1 { rx *= lambda.squareRoot(); ry *= lambda.squareRoot() }
        let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
        let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1
        var factor = denominator == 0 ? 0 : max(0, numerator / denominator).squareRoot()
        if large == sweep { factor = -factor }
        let cxp = factor * rx * y1 / ry, cyp = -factor * ry * x1 / rx
        let cx = cosPhi * cxp - sinPhi * cyp + (p1.0 + p2.0) / 2
        let cy = sinPhi * cxp + cosPhi * cyp + (p1.1 + p2.1) / 2
        func angle(_ ux: Double, _ uy: Double, _ vx: Double, _ vy: Double) -> Double {
            atan2(ux * vy - uy * vx, ux * vx + uy * vy)
        }
        let theta = angle(1, 0, (x1 - cxp) / rx, (y1 - cyp) / ry)
        var delta = angle((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry)
        if !sweep && delta > 0 { delta -= 2 * .pi } else if sweep && delta < 0 { delta += 2 * .pi }
        let segments = max(1, Int((abs(delta) / (.pi / 2) - 1e-9).rounded(.up)))
        let step = delta / Double(segments)
        let k = 4 / 3 * tan(step / 4)
        func point(_ t: Double) -> (Double, Double) {
            (cx + rx * cos(t) * cosPhi - ry * sin(t) * sinPhi, cy + rx * cos(t) * sinPhi + ry * sin(t) * cosPhi)
        }
        func derivative(_ t: Double) -> (Double, Double) {
            (-rx * sin(t) * cosPhi - ry * cos(t) * sinPhi, -rx * sin(t) * sinPhi + ry * cos(t) * cosPhi)
        }
        var out: [SVGCommand] = []
        var t = theta
        for index in 0..<segments {
            let start = point(t), end = index == segments - 1 ? p2 : point(t + step)
            let d0 = derivative(t), d1 = derivative(t + step)
            out.append(.cubic(start.0 + k * d0.0, start.1 + k * d0.1, end.0 - k * d1.0, end.1 - k * d1.1, end.0, end.1))
            t += step
        }
        return out
    }

    /// Reads numbers and arc flags from path data, which may omit separators (`1.5.5`, `-1-2`, `a1 1 0 01.5.5`).
    struct Scanner {
        let bytes: [UInt8]
        var index = 0
        init(_ bytes: [UInt8]) { self.bytes = bytes }

        static func isCommand(_ byte: UInt8) -> Bool {
            switch byte | 0x20 {
            case UInt8(ascii: "m"), UInt8(ascii: "l"), UInt8(ascii: "h"), UInt8(ascii: "v"), UInt8(ascii: "c"),
                 UInt8(ascii: "s"), UInt8(ascii: "q"), UInt8(ascii: "t"), UInt8(ascii: "a"), UInt8(ascii: "z"):
                return byte != UInt8(ascii: "e") && byte != UInt8(ascii: "E")
            default: return false
            }
        }
        func peek() -> UInt8? { index < bytes.count ? bytes[index] : nil }
        mutating func advance() { index += 1 }
        mutating func skipSeparators() {
            while let byte = peek(), byte == 0x20 || byte == 0x2C || byte == 0x09 || byte == 0x0A || byte == 0x0D { index += 1 }
        }
        mutating func numbers(_ count: Int) -> [Double]? {
            var values: [Double] = []
            values.reserveCapacity(count)
            for _ in 0..<count {
                guard let value = number() else { return nil }
                values.append(value)
            }
            return values
        }
        mutating func flag() -> Bool? {
            skipSeparators()
            guard let byte = peek(), byte == UInt8(ascii: "0") || byte == UInt8(ascii: "1") else { return nil }
            advance()
            return byte == UInt8(ascii: "1")
        }
        mutating func number() -> Double? {
            skipSeparators()
            let start = index
            if let sign = peek(), sign == UInt8(ascii: "-") || sign == UInt8(ascii: "+") { advance() }
            var digits = false, dot = false
            while let byte = peek() {
                if byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9") {
                    digits = true
                } else if byte == UInt8(ascii: "."), !dot {
                    dot = true
                } else {
                    break
                }
                advance()
            }
            guard digits else { index = start; return nil }
            if let e = peek(), e == UInt8(ascii: "e") || e == UInt8(ascii: "E") {
                let mark = index
                advance()
                if let sign = peek(), sign == UInt8(ascii: "-") || sign == UInt8(ascii: "+") { advance() }
                var exponent = false
                while let byte = peek(), byte >= UInt8(ascii: "0") && byte <= UInt8(ascii: "9") { exponent = true; advance() }
                if !exponent { index = mark }
            }
            return Double(String(decoding: bytes[start..<index], as: UTF8.self))
        }
    }
}
