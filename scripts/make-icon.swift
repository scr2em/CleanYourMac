import AppKit
import Foundation

let destination = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = points * scale
        let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        let context = NSGraphicsContext(bitmapImageRep: bitmap)!
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        context.cgContext.scaleBy(x: CGFloat(pixels) / 1024, y: CGFloat(pixels) / 1024)
        let tile = NSBezierPath(roundedRect: NSRect(x: 60, y: 60, width: 904, height: 904), xRadius: 205, yRadius: 205)
        NSGradient(starting: NSColor(srgbRed: 0.12, green: 0.48, blue: 0.44, alpha: 1), ending: NSColor(srgbRed: 0.04, green: 0.19, blue: 0.23, alpha: 1))!.draw(in: tile, angle: 65)
        NSColor(srgbRed: 0.91, green: 0.99, blue: 0.96, alpha: 1).setFill()
        let tab = NSBezierPath(roundedRect: NSRect(x: 212, y: 526, width: 270, height: 170), xRadius: 35, yRadius: 35)
        tab.fill()
        NSBezierPath(roundedRect: NSRect(x: 212, y: 302, width: 600, height: 345), xRadius: 55, yRadius: 55).fill()
        NSColor(srgbRed: 0.06, green: 0.32, blue: 0.32, alpha: 1).setFill()
        for (index, width) in [260.0, 190.0, 120.0].enumerated() {
            NSBezierPath(roundedRect: NSRect(x: 277, y: 528 - Double(index) * 68, width: width, height: 24), xRadius: 12, yRadius: 12).fill()
        }
        NSColor(srgbRed: 0.42, green: 0.91, blue: 0.72, alpha: 1).setFill()
        NSBezierPath(ovalIn: NSRect(x: 611, y: 249, width: 210, height: 210)).fill()
        NSColor(srgbRed: 0.04, green: 0.25, blue: 0.25, alpha: 1).setStroke()
        let check = NSBezierPath()
        check.move(to: NSPoint(x: 667, y: 351)); check.line(to: NSPoint(x: 702, y: 315)); check.line(to: NSPoint(x: 769, y: 389))
        check.lineWidth = 22; check.lineCapStyle = .round; check.lineJoinStyle = .round; check.stroke()
        NSGraphicsContext.restoreGraphicsState()
        let name = "icon_\(points)x\(points)" + (scale == 2 ? "@2x" : "") + ".png"
        try bitmap.representation(using: .png, properties: [:])!.write(to: destination.appendingPathComponent(name))
    }
}
