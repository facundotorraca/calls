import AVFoundation
import CoreAudio

/// Frames written by an audio callback, readable from any thread.
final class FrameCounter {
    private let lock = NSLock()
    private var frames: Int64 = 0
    private var closed = false

    /// Runs `write` unless the recording was closed; counts the frames.
    func record(_ count: AVAudioFrameCount, _ write: () -> Void) {
        lock.withLock {
            guard !closed else { return }
            write()
            frames += Int64(count)
        }
    }

    func close() { lock.withLock { closed = true } }
    var total: Int64 { lock.withLock { frames } }
}

/// Records what an app plays (the other side of the call) with a CoreAudio process tap.
/// Needs the "System Audio Recording" permission, not screen recording.
final class SystemRecorder {
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    private var file: AVAudioFile?
    private let counter = FrameCounter()
    private let queue = DispatchQueue(label: "calls-audio.system", qos: .userInitiated)

    var framesWritten: Int64 { counter.total }

    /// Taps the given processes, or every process but ourselves if the list is empty.
    func start(processes: [AudioObjectID], url: URL, onLevel: @escaping (Float) -> Void) throws {
        let description = processes.isEmpty
            ? CATapDescription(stereoGlobalTapButExcludeProcesses: ownProcessObject().map { [$0] } ?? [])
            : CATapDescription(stereoMixdownOfProcesses: processes)
        description.uuid = UUID()
        description.name = "calls"
        description.isPrivate = true
        description.muteBehavior = .unmuted
        try check(AudioHardwareCreateProcessTap(description, &tapID), "creating the audio tap")

        // A tap is read through an aggregate device that wraps the current output device.
        let output = property(systemObject, kAudioHardwarePropertyDefaultSystemOutputDevice, default: AudioObjectID(0)) ?? 0
        guard let outputUID = stringProperty(output, kAudioDevicePropertyDeviceUID) else {
            throw AudioError(description: "no output device")
        }
        let aggregate: [String: Any] = [
            kAudioAggregateDeviceNameKey: "calls-tap",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID]],
            kAudioAggregateDeviceTapListKey: [
                [kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: description.uuid.uuidString]
            ],
        ]
        try check(AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &aggregateID), "creating the aggregate device")

        guard var streamDescription = property(tapID, kAudioTapPropertyFormat, default: AudioStreamBasicDescription()),
              let format = AVAudioFormat(streamDescription: &streamDescription)
        else { throw AudioError(description: "could not read the tap format") }

        let file = try AVAudioFile(
            forWriting: url,
            settings: aacSettings(sampleRate: format.sampleRate, channels: format.channelCount),
            commonFormat: format.commonFormat,
            interleaved: format.isInterleaved
        )
        self.file = file

        let counter = self.counter
        try check(AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, queue) { _, input, _, _, _ in
            guard let buffer = AVAudioPCMBuffer(pcmFormat: format, bufferListNoCopy: input, deallocator: nil) else { return }
            counter.record(buffer.frameLength) { try? file.write(from: buffer) }
            onLevel(rms(buffer))
        }, "creating the capture callback")
        try check(AudioDeviceStart(aggregateID, procID), "starting the capture")
    }

    func stop() {
        // Teardown talks to coreaudiod, which can hang (e.g. while a permission prompt is
        // open). Never let that block finishing the recording.
        let (aggregateID, procID, tapID) = (self.aggregateID, self.procID, self.tapID)
        let done = DispatchSemaphore(value: 0)
        DispatchQueue.global(qos: .userInitiated).async {
            if aggregateID != kAudioObjectUnknown {
                AudioDeviceStop(aggregateID, procID)
                if let procID { AudioDeviceDestroyIOProcID(aggregateID, procID) }
                AudioHardwareDestroyAggregateDevice(aggregateID)
            }
            if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
            done.signal()
        }
        if done.wait(timeout: .now() + 3) == .timedOut {
            emit(["event": "error", "message": "system audio did not stop cleanly"])
        }
        self.aggregateID = AudioObjectID(kAudioObjectUnknown)
        self.tapID = AudioObjectID(kAudioObjectUnknown)
        self.procID = nil

        counter.close()
        file?.close()
        file = nil
    }
}
