// swift-tools-version:5.9
import PackageDescription

// Shared Swift runtime for rux.
//
// - `CRux`: C header describing the ABI between Swift and Rust (rux.h).
// - `RuxRuntime`: `RuxHost` — applies mutations from Rust to UIKit.
//
// The `rux_start` / `rux_dispatch` / `rux_stop` symbols live in the app's static library
// (the xcframework produced by `rux build ios`) and are resolved when the app is linked.
let package = Package(
    name: "RuxRuntime",
    platforms: [.iOS(.v15)],
    products: [
        .library(name: "RuxRuntime", targets: ["RuxRuntime"]),
    ],
    targets: [
        .target(name: "CRux", path: "Sources/CRux"),
        .target(name: "RuxRuntime", dependencies: ["CRux"], path: "Sources/RuxRuntime"),
    ]
)
