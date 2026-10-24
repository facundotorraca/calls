// calls-audio: long-running sidecar of the Calls app.
//
// stdout, one JSON object per line:
//   {"event":"ready"}
//   {"event":"audio_apps","apps":[{"pid":1,"bundle_id":"us.zoom.xos","input":true,"output":true}]}  every second
//   {"event":"levels","mic":0.1,"system":0.3}                                                       while recording
//   {"event":"recording_started"}
//   {"event":"recording_stopped","files":["mic.m4a","system.m4a"],"offsets_ms":{"mic.m4a":0,"system.m4a":0}}
//       files: tracks that got audio; offsets_ms: when each one started, relative to the recording
//   {"event":"error","message":"..."}
// stdin, one JSON object per line:
//   {"cmd":"start","dir":"/path/to/call","bundle_prefix":"us.zoom"}   bundle_prefix null = all apps
//   {"cmd":"stop"}
// Closing stdin (the app quit or crashed) stops any recording cleanly and exits.
import AVFoundation
import Foundation

let micFile = "mic.m4a"
let systemFile = "system.m4a"

func emit(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object) else { return }
    FileHandle.standardOutput.write(data + Data("\n".utf8))
}

/// Levels are written from audio threads and read from the main thread.
final class Levels {
    private let lock = NSLock()
    private var mic: Float = 0
    private var system: Float = 0

    func setMic(_ value: Float) { lock.withLock { mic = max(mic, value) } }
    func setSystem(_ value: Float) { lock.withLock { system = max(system, value) } }

    /// Peak since the last call.
    func take() -> (Float, Float) {
        lock.withLock {
            defer { mic = 0; system = 0 }
            return (mic, system)
        }
    }
}

final class Session {
    private var mic: MicRecorder?
    private var system: SystemRecorder?
    private var dir: URL?
    private var startedAt = Date()
    private var offsets: [String: Int] = [:]
    private let levels = Levels()

    var isRecording: Bool { dir != nil }

    func handle(_ line: String) {
        guard let data = line.data(using: .utf8),
              let command = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let cmd = command["cmd"] as? String
        else { return emit(["event": "error", "message": "invalid command: \(line)"]) }

        switch cmd {
        case "start":
            guard let dir = command["dir"] as? String else { return emit(["event": "error", "message": "start needs a dir"]) }
            start(dir: URL(fileURLWithPath: dir), bundlePrefix: command["bundle_prefix"] as? String)
        case "stop":
            stop()
        default:
            emit(["event": "error", "message": "unknown command: \(cmd)"])
        }
    }

    private func start(dir: URL, bundlePrefix: String?) {
        guard !isRecording else { return emit(["event": "error", "message": "already recording"]) }
        self.dir = dir
        startedAt = Date()
        offsets = [systemFile: 0]

        // The other side is what matters most: start it even if the mic fails.
        do {
            let processes = bundlePrefix.map(processObjects(bundlePrefix:)) ?? []
            let system = SystemRecorder()
            try system.start(processes: processes, url: dir.appendingPathComponent(systemFile), onLevel: levels.setSystem)
            self.system = system
            // No callbacks at all (not even silence) means macOS is not letting us capture.
            DispatchQueue.main.asyncAfter(deadline: .now() + 3) {
                guard self.dir == dir, system.framesWritten == 0 else { return }
                emit(["event": "error", "message": "no system audio: check Privacy & Security > Screen & System Audio Recording"])
            }
        } catch {
            emit(["event": "error", "message": "system audio: \(error)"])
        }

        MicRecorder.requestPermission { granted in
            guard self.dir == dir else { return }
            guard granted else { return emit(["event": "error", "message": "microphone permission denied"]) }
            do {
                let mic = MicRecorder()
                try mic.start(url: dir.appendingPathComponent(micFile), onLevel: self.levels.setMic)
                self.mic = mic
                // The permission prompt may have delayed the mic: keep both tracks aligned.
                self.offsets[micFile] = Int(Date().timeIntervalSince(self.startedAt) * 1000)
            } catch {
                emit(["event": "error", "message": "microphone: \(error)"])
            }
        }
        emit(["event": "recording_started"])
    }

    func stop() {
        guard isRecording else { return }
        mic?.stop()
        system?.stop()
        var files: [String] = []
        if (mic?.framesWritten ?? 0) > 0 { files.append(micFile) }
        if (system?.framesWritten ?? 0) > 0 { files.append(systemFile) }
        mic = nil
        system = nil
        self.dir = nil

        emit(["event": "recording_stopped", "files": files, "offsets_ms": offsets.filter { files.contains($0.key) }])
    }

    func reportProcesses() {
        emit(["event": "audio_apps", "apps": activeAudioProcesses().map(\.json)])
    }

    func reportLevels() {
        guard isRecording else { return }
        let (mic, system) = levels.take()
        emit(["event": "levels", "mic": mic, "system": system])
    }
}

let session = Session()

Thread {
    while let line = readLine() {
        DispatchQueue.main.async { session.handle(line) }
    }
    DispatchQueue.main.async {
        session.stop()
        exit(0)
    }
}.start()

signal(SIGTERM, SIG_IGN)
let sigterm = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
sigterm.setEventHandler {
    session.stop()
    exit(0)
}
sigterm.resume()

Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in session.reportProcesses() }
Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { _ in session.reportLevels() }

emit(["event": "ready"])
session.reportProcesses()
RunLoop.main.run()
