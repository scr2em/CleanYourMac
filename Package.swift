// swift-tools-version: 6.0
import Foundation
import PackageDescription

// The Rust core is built first by scripts/build-core.sh into target/cym/libcym_core.a.
let coreLibrary = URL(fileURLWithPath: #filePath).deletingLastPathComponent().appendingPathComponent("target/cym").path

let package = Package(
    name: "CleanYourMac",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "CleanYourMac", targets: ["CleanYourMac"]),
        .library(name: "CleanYourMacCore", targets: ["CleanYourMacCore"]),
    ],
    targets: [
        .systemLibrary(name: "CCymCore", path: "Sources/CCymCore"),
        .target(
            name: "CleanYourMacCore",
            dependencies: ["CCymCore"],
            linkerSettings: [
                .unsafeFlags(["-L", coreLibrary]),
                .linkedLibrary("cym_core"),
                .linkedFramework("AppKit"),
                .linkedFramework("Foundation"),
            ]
        ),
        .target(name: "CleanYourMacDesignSystem"),
        .target(name: "CleanYourMacUI", dependencies: ["CleanYourMacCore", "CleanYourMacDesignSystem"]),
        .executableTarget(name: "CleanYourMac", dependencies: ["CleanYourMacUI", "CleanYourMacDesignSystem"]),
        .testTarget(name: "CleanYourMacTests", dependencies: ["CleanYourMacCore", "CleanYourMacUI"]),
    ]
)
