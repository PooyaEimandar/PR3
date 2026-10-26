// swift-tools-version:5.5
// The Apple half of pr3-speech: on-device recognition (Speech) and voices
// (AVSpeechSynthesizer), behind a small C interface (Source/PR3Speech/Speech.swift).
// Built and linked by build.rs.
import PackageDescription

let package = Package(
    name: "PR3Speech",
    platforms: [.macOS(.v11), .iOS(.v13)],
    products: [.library(name: "PR3Speech", type: .static, targets: ["PR3Speech"])],
    targets: [.target(name: "PR3Speech", path: "Source/PR3Speech")]
)
