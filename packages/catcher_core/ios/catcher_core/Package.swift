// swift-tools-version: 5.9
// The swift-tools-version declares the minimum version of Swift required to build this package.

import PackageDescription

let package = Package(
    name: "catcher_core",
    platforms: [
        .iOS("15.0")
    ],
    products: [
        .library(name: "catcher-core", targets: ["catcher_core"])
    ],
    targets: [
        .target(
            name: "catcher_core",
            dependencies: [
                .target(name: "catcher_ffi")
            ]
        ),
        .binaryTarget(
            name: "catcher_ffi",
            path: "catcher_ffi.xcframework"
        )
    ]
)
