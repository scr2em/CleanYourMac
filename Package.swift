// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CleanYourMac",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "CleanYourMac", targets: ["CleanYourMac"]),
        .executable(name: "cym", targets: ["CleanYourMacCLI"]),
        .library(name: "CleanYourMacCore", targets: ["CleanYourMacCore"]),
    ],
    targets: [
        .target(name: "CleanYourMacCore"),
        .target(name: "CleanYourMacPlatform", dependencies: ["CleanYourMacCore"]),
        .target(name: "CleanYourMacModules", dependencies: ["CleanYourMacCore", "CleanYourMacPlatform"]),
        .target(name: "CleanYourMacDesignSystem"),
        .target(name: "CleanYourMacUI", dependencies: ["CleanYourMacCore", "CleanYourMacPlatform", "CleanYourMacModules", "CleanYourMacDesignSystem"]),
        .executableTarget(name: "CleanYourMac", dependencies: ["CleanYourMacUI", "CleanYourMacDesignSystem"]),
        .executableTarget(name: "CleanYourMacCLI", dependencies: ["CleanYourMacCore", "CleanYourMacPlatform", "CleanYourMacModules"]),
        .testTarget(name: "CleanYourMacTests", dependencies: ["CleanYourMacCore", "CleanYourMacPlatform", "CleanYourMacModules", "CleanYourMacUI"]),
    ]
)
