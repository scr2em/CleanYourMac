import CleanYourMacDesignSystem
import Testing

@Test func svgPathDataBecomesAbsoluteCommands() throws {
    // Relative commands, H/V, Z returning to the subpath start, a reflected S, an arc and
    // numbers without separators, as minified icon files write them.
    let commands = SVGPathParser.parse("M2 2h4v4H2zm10 0c1 0 2 1 2 2s-1 2-2 2a2 2 0 01-2-2l.5-.5")
    #expect(Array(commands.prefix(8)) == [
        .move(2, 2), .line(6, 2), .line(6, 6), .line(2, 6), .close,
        .move(12, 2), .cubic(13, 2, 14, 3, 14, 4), .cubic(14, 5, 13, 6, 12, 6),
    ])
    guard case let .cubic(_, _, _, _, x, y) = commands[8] else { Issue.record("The arc should become a curve"); return }
    #expect(x == 10 && y == 4)
    #expect(commands.last == .line(10.5, 3.5))
    #expect(SVGPathParser.parse("M1e1-2E-1L3,4") == [.move(10, -0.2), .line(3, 4)])
}

@Test func everyBundledLogoLoadsWithItsNameAndColour() throws {
    #expect(BrandCatalog.slugs.count >= 50)
    #expect(BrandCatalog.name("flutter") == "Flutter")
    #expect(BrandCatalog.contains("nextdotjs"))
    let svg = #"<svg fill="#61DAFB" role="img" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><title>React</title><path d="M0 0h24v24H0z"/></svg>"#
    let icon = try #require(SVGIcon(svg: svg))
    #expect(icon.fill == 0x61DAFB)
    #expect(icon.title == "React")
    #expect(icon.commands.count == 5)
}
