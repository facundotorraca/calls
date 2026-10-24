import AVFoundation

/// Records the default microphone (your side of the call) as mono.
final class MicRecorder {
    private let engine = AVAudioEngine()
    private var file: AVAudioFile?
    private let counter = FrameCounter()

    var framesWritten: Int64 { counter.total }

    static func requestPermission(_ done: @escaping (Bool) -> Void) {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: done(true)
        case .notDetermined: AVCaptureDevice.requestAccess(for: .audio) { granted in DispatchQueue.main.async { done(granted) } }
        default: done(false)
        }
    }

    func start(url: URL, onLevel: @escaping (Float) -> Void) throws {
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.channelCount > 0, format.sampleRate > 0 else { throw AudioError(description: "no microphone available") }

        // Some mics expose several channels; a voice track only needs one.
        guard let mono = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: format.sampleRate, channels: 1, interleaved: false)
        else { throw AudioError(description: "unsupported microphone format") }
        let converter = format.channelCount == 1 ? nil : AVAudioConverter(from: format, to: mono)
        converter?.downmix = true

        let file = try AVAudioFile(
            forWriting: url,
            settings: aacSettings(sampleRate: format.sampleRate, channels: 1),
            commonFormat: .pcmFormatFloat32,
            interleaved: false
        )
        self.file = file

        let counter = self.counter
        input.installTap(onBus: 0, bufferSize: 4096, format: format) { buffer, _ in
            var out = buffer
            if let converter {
                guard let converted = AVAudioPCMBuffer(pcmFormat: mono, frameCapacity: buffer.frameLength) else { return }
                guard (try? converter.convert(to: converted, from: buffer)) != nil else { return }
                out = converted
            }
            counter.record(out.frameLength) { try? file.write(from: out) }
            onLevel(rms(out))
        }
        engine.prepare()
        try engine.start()
    }

    func stop() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        counter.close()
        file?.close()
        file = nil
    }
}
