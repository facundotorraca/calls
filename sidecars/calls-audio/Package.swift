// swift-tools-version:6.0
// Audio sidecar: detects apps using the microphone and records calls (mic + app audio)
// with Apple's own APIs (CoreAudio process taps, AVAudioEngine). Talks JSON lines over stdio.
import PackageDescription

let package = Package(
    name: "calls-audio",
    platforms: [.macOS(.v15)],
    targets: [
        .executableTarget(name: "calls-audio", path: "Sources/calls-audio")
    ],
    swiftLanguageModes: [.v5]
)
