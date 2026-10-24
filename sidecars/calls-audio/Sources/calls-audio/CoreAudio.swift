import AVFoundation
import CoreAudio

struct AudioError: Error, CustomStringConvertible {
    let description: String
}

func check(_ status: OSStatus, _ what: String) throws {
    if status != noErr { throw AudioError(description: "\(what) failed (OSStatus \(status))") }
}

private func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
}

func property<T: BitwiseCopyable>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, default value: T) -> T? {
    var address = address(selector)
    var size = UInt32(MemoryLayout<T>.size)
    var value = value
    return AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value) == noErr ? value : nil
}

func stringProperty(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String? {
    var address = address(selector)
    var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
    var value: Unmanaged<CFString>?
    guard AudioObjectGetPropertyData(object, &address, 0, nil, &size, &value) == noErr else { return nil }
    return value?.takeRetainedValue() as String?
}

func objectList(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> [AudioObjectID] {
    var address = address(selector)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(object, &address, 0, nil, &size) == noErr, size > 0 else { return [] }
    var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(object, &address, 0, nil, &size, &ids) == noErr else { return [] }
    return ids
}

let systemObject = AudioObjectID(kAudioObjectSystemObject)

/// A process with an audio client, as seen by CoreAudio (macOS 14.2+).
struct AudioProcess {
    let objectID: AudioObjectID
    let pid: pid_t
    let bundleID: String
    let input: Bool
    let output: Bool

    var json: [String: Any] { ["pid": pid, "bundle_id": bundleID, "input": input, "output": output] }
}

/// Processes currently capturing (input) or playing (output) audio, excluding ourselves.
func activeAudioProcesses() -> [AudioProcess] {
    objectList(systemObject, kAudioHardwarePropertyProcessObjectList).compactMap { id in
        guard let pid = property(id, kAudioProcessPropertyPID, default: pid_t(0)), pid != getpid() else { return nil }
        let input = (property(id, kAudioProcessPropertyIsRunningInput, default: UInt32(0)) ?? 0) != 0
        let output = (property(id, kAudioProcessPropertyIsRunningOutput, default: UInt32(0)) ?? 0) != 0
        guard input || output else { return nil }
        let bundleID = stringProperty(id, kAudioProcessPropertyBundleID) ?? ""
        return AudioProcess(objectID: id, pid: pid, bundleID: bundleID, input: input, output: output)
    }
}

/// All audio process objects whose bundle id starts with `prefix` (an app and its helpers).
func processObjects(bundlePrefix prefix: String) -> [AudioObjectID] {
    objectList(systemObject, kAudioHardwarePropertyProcessObjectList).filter { id in
        stringProperty(id, kAudioProcessPropertyBundleID)?.hasPrefix(prefix) ?? false
    }
}

func ownProcessObject() -> AudioObjectID? {
    var address = address(kAudioHardwarePropertyTranslatePIDToProcessObject)
    var pid = getpid()
    var object = AudioObjectID(kAudioObjectUnknown)
    var size = UInt32(MemoryLayout<AudioObjectID>.size)
    let status = AudioObjectGetPropertyData(systemObject, &address, UInt32(MemoryLayout<pid_t>.size), &pid, &size, &object)
    return status == noErr && object != kAudioObjectUnknown ? object : nil
}

/// AAC in an .m4a container: ~30 MB per hour of call instead of ~1 GB of raw PCM.
func aacSettings(sampleRate: Double, channels: AVAudioChannelCount) -> [String: Any] {
    [
        AVFormatIDKey: kAudioFormatMPEG4AAC,
        AVSampleRateKey: sampleRate,
        AVNumberOfChannelsKey: channels,
        AVEncoderBitRateKey: channels > 1 ? 128_000 : 64_000,
    ]
}

/// Root mean square of the first channel, for level meters.
func rms(_ buffer: AVAudioPCMBuffer) -> Float {
    guard let data = buffer.floatChannelData, buffer.frameLength > 0 else { return 0 }
    let samples = UnsafeBufferPointer(start: data[0], count: Int(buffer.frameLength) * buffer.stride)
    let sum = samples.reduce(Float(0)) { $0 + $1 * $1 }
    return (sum / Float(samples.count)).squareRoot()
}
