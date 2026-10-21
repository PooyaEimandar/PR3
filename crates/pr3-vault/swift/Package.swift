// swift-tools-version:5.5
// The Apple half of pr3-vault: Secure Enclave keys and LocalAuthentication, behind a
// small C interface (Sources/PR3Vault/Vault.swift). Built and linked by build.rs.
import PackageDescription

let package = Package(
    name: "PR3Vault",
    platforms: [.macOS(.v11), .iOS(.v13)],
    products: [.library(name: "PR3Vault", type: .static, targets: ["PR3Vault"])],
    targets: [.target(name: "PR3Vault", path: "Sources/PR3Vault")]
)
