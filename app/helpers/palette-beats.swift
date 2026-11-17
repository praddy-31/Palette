// palette-beats: the bar's beat lines (ROADMAP 20). Started by Palette's app (so macOS asks for "System Audio
// Recording" for Palette, once), it taps what the Mac plays while the now-playing feeder says something is
// playing, splits it into a few frequency bands and sets the bar's `beat.N` items' heights through SketchyBar's
// mach port: one message per frame, no process per frame. Nothing is recorded or stored.
//
//   palette-beats --bars 6 --min 2 --max 18 --state ~/Library/Caches/palette/nowplaying.state
//   palette-beats --selftest          (spectrum of a test tone, the message format; no audio, no bar)
//
// The audio tap and the spectrum come from the owner's Widgets app (AudioTap.swift, Spectrum.swift).
// Build: swiftc -O app/helpers/palette-beats.swift -o app/binaries/palette-beats-<triple> (app/sidecar.sh).

import Accelerate
import AudioToolbox
import CoreAudio
import Darwin
import Foundation

// ---------------------------------------------------------------- spectrum (from Widgets)

final class Spectrum {
    let size = 1024
    let bands: Int
    private let fft: vDSP.FFT<DSPSplitComplex>?
    private let window: [Float]
    private var peaks: [Float]
    private var shown: [Double]
    let ranges: [Range<Int>]

    init(bands: Int, sampleRate: Double) {
        self.bands = max(bands, 1)
        fft = vDSP.FFT(log2n: 10, radix: .radix2, ofType: DSPSplitComplex.self)
        window = vDSP.window(ofType: Float.self, usingSequence: .hanningDenormalized, count: 1024, isHalfWindow: false)
        peaks = Array(repeating: 1e-4, count: self.bands)
        shown = Array(repeating: 0, count: self.bands)
        ranges = Self.bandRanges(bands: self.bands, sampleRate: sampleRate, size: 1024)
    }

    /// Log-spaced bands from 40 Hz to 12 kHz (or Nyquist), each at least one bin wide.
    static func bandRanges(bands: Int, sampleRate: Double, size: Int) -> [Range<Int>] {
        let lo = 40.0, hi = min(12_000, sampleRate / 2 * 0.95), binHz = sampleRate / Double(size)
        var out: [Range<Int>] = []
        var start = max(Int(lo / binHz), 1)
        for b in 0..<bands {
            let edge = lo * pow(hi / lo, Double(b + 1) / Double(bands))
            let end = max(Int(edge / binHz), start + 1)
            out.append(start..<min(end, size / 2))
            start = min(end, size / 2 - 1)
        }
        return out
    }

    /// Levels 0-1 per band (low to high) for one block of `size` mono samples: each band against its own slowly
    /// falling peak, up fast and down slower, like an equaliser.
    func levels(_ samples: [Float]) -> [Double] {
        guard samples.count >= size, let fft else { return shown }
        let x = vDSP.multiply(Array(samples.prefix(size)), window)
        var re = [Float](repeating: 0, count: size / 2), im = [Float](repeating: 0, count: size / 2)
        var mags = [Float](repeating: 0, count: size / 2)
        re.withUnsafeMutableBufferPointer { rp in
            im.withUnsafeMutableBufferPointer { ip in
                var split = DSPSplitComplex(realp: rp.baseAddress!, imagp: ip.baseAddress!)
                x.withUnsafeBufferPointer { xp in
                    xp.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: size / 2) {
                        vDSP_ctoz($0, 2, &split, 1, vDSP_Length(size / 2))
                    }
                }
                fft.forward(input: split, output: &split)
                vDSP.squareMagnitudes(split, result: &mags)
            }
        }
        for (b, r) in ranges.enumerated() {
            let energy = r.isEmpty ? 0 : mags[r].reduce(0, +) / Float(r.count)
            let v = sqrt(energy)
            peaks[b] = max(peaks[b] * 0.997, v, 1e-4)
            let level = Double(min(v / peaks[b], 1))
            let target = level * level
            shown[b] = target > shown[b] ? target : shown[b] * 0.82 + target * 0.18
        }
        return shown
    }
}

// ---------------------------------------------------------------- SketchyBar's mach port

/// Sends a command to SketchyBar the way its own CLI does (`git.felix.sketchybar`): the arguments, each ending in
/// a NUL, plus a final NUL, as one out-of-line block. No reply is asked for (a `--set` has none worth waiting for).
final class SketchyBar {
    private var port: mach_port_t = 0

    static func bytes(_ args: [String]) -> [UInt8] {
        var out: [UInt8] = []
        for a in args { out += Array(a.utf8); out.append(0) }
        out.append(0)
        return out
    }

    private func lookUp() -> mach_port_t {
        var bootstrap: mach_port_t = 0
        guard task_get_special_port(mach_task_self_, TASK_BOOTSTRAP_PORT, &bootstrap) == KERN_SUCCESS else { return 0 }
        // <servers/bootstrap.h> isn't visible to Swift: the function from libSystem by name.
        typealias LookUp = @convention(c) (mach_port_t, UnsafePointer<CChar>, UnsafeMutablePointer<mach_port_t>) -> kern_return_t
        guard let f = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "bootstrap_look_up") else { return 0 }   // RTLD_DEFAULT
        var p: mach_port_t = 0
        return unsafeBitCast(f, to: LookUp.self)(bootstrap, "git.felix.sketchybar", &p) == KERN_SUCCESS ? p : 0
    }

    /// False when SketchyBar isn't there (it's looked up again next time).
    @discardableResult
    func send(_ args: [String]) -> Bool {
        if port == 0 { port = lookUp() }
        guard port != 0 else { return false }
        var payload = Self.bytes(args)
        // mach_msg_header_t (24) + descriptor count (4) + mach_msg_ool_descriptor_t (16), 4-byte packed: 44 bytes.
        let size = 44
        let ok = payload.withUnsafeMutableBytes { data -> Bool in
            let msg = UnsafeMutableRawPointer.allocate(byteCount: size, alignment: 8)
            defer { msg.deallocate() }
            msg.initializeMemory(as: UInt8.self, repeating: 0, count: size)
            let bits = UInt32(MACH_MSG_TYPE_COPY_SEND) | MACH_MSGH_BITS_COMPLEX
            msg.storeBytes(of: bits, toByteOffset: 0, as: UInt32.self)               // msgh_bits
            msg.storeBytes(of: UInt32(size), toByteOffset: 4, as: UInt32.self)       // msgh_size
            msg.storeBytes(of: port, toByteOffset: 8, as: UInt32.self)               // msgh_remote_port
            msg.storeBytes(of: UInt32(1), toByteOffset: 24, as: UInt32.self)         // msgh_descriptor_count
            msg.storeBytes(of: UInt(bitPattern: data.baseAddress), toByteOffset: 28, as: UInt.self)   // address
            // deallocate:8 copy:8 pad1:8 type:8, then size
            let flags = UInt32(0) | (UInt32(MACH_MSG_VIRTUAL_COPY) << 8) | (UInt32(MACH_MSG_OOL_DESCRIPTOR) << 24)
            msg.storeBytes(of: flags, toByteOffset: 36, as: UInt32.self)
            msg.storeBytes(of: UInt32(data.count), toByteOffset: 40, as: UInt32.self)
            let r = mach_msg(msg.assumingMemoryBound(to: mach_msg_header_t.self), MACH_SEND_MSG | MACH_SEND_TIMEOUT,
                             mach_msg_size_t(size), 0, mach_port_t(MACH_PORT_NULL), 50, mach_port_t(MACH_PORT_NULL))
            return r == MACH_MSG_SUCCESS
        }
        if !ok { port = 0 }
        return ok
    }
}

// ---------------------------------------------------------------- the audio tap (from Widgets)

final class AudioTap: @unchecked Sendable {
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    private let queue = DispatchQueue(label: "palette.beats", qos: .userInitiated)
    private var spectrum: Spectrum?
    private var buffer: [Float] = []
    private var lastSent = 0.0
    let bands: Int
    /// Levels 0-1, low to high, at most 20 times a second, on `queue`.
    var onLevels: (([Double]) -> Void)?

    init(bands: Int) { self.bands = bands }

    var isRunning: Bool { procID != nil }

    func start() throws {
        guard procID == nil else { return }
        let desc = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
        desc.uuid = UUID()
        desc.isPrivate = true
        desc.muteBehavior = .unmuted
        try check(AudioHardwareCreateProcessTap(desc, &tapID), "create the audio tap")
        var format = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        var addr = AudioObjectPropertyAddress(mSelector: kAudioTapPropertyFormat, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        try check(AudioObjectGetPropertyData(tapID, &addr, 0, nil, &size, &format), "read the tap's format")
        spectrum = Spectrum(bands: bands, sampleRate: format.mSampleRate > 0 ? format.mSampleRate : 48_000)
        let output = try Self.defaultOutputUID()
        let agg: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Palette beat lines",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: output,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: output]],
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: false, kAudioSubTapUIDKey: desc.uuid.uuidString]],
        ]
        try check(AudioHardwareCreateAggregateDevice(agg as CFDictionary, &aggregateID), "create the aggregate device")
        let interleaved = format.mFormatFlags & kAudioFormatFlagIsNonInterleaved == 0
        let channels = max(Int(format.mChannelsPerFrame), 1)
        try check(AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, queue) { [weak self] _, input, _, _, _ in
            self?.take(input, interleaved: interleaved, channels: channels)
        }, "start reading audio")
        try check(AudioDeviceStart(aggregateID, procID), "start the device")
    }

    func stop() {
        if let procID {
            AudioDeviceStop(aggregateID, procID)
            AudioDeviceDestroyIOProcID(aggregateID, procID)
        }
        procID = nil
        if aggregateID != kAudioObjectUnknown { AudioHardwareDestroyAggregateDevice(aggregateID) }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        aggregateID = AudioObjectID(kAudioObjectUnknown)
        tapID = AudioObjectID(kAudioObjectUnknown)
        queue.sync { buffer.removeAll() }
    }

    private func take(_ list: UnsafePointer<AudioBufferList>, interleaved: Bool, channels: Int) {
        let abl = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: list))
        guard let first = abl.first, let data = first.mData else { return }
        let frames = Int(first.mDataByteSize) / MemoryLayout<Float>.size / (interleaved ? channels : 1)
        let p = data.assumingMemoryBound(to: Float.self)
        let stride = interleaved ? channels : 1
        for i in 0..<frames { buffer.append(p[i * stride]) }   // the left channel: enough for levels
        guard let spectrum, buffer.count >= spectrum.size else { return }
        let block = Array(buffer.suffix(spectrum.size))
        buffer.removeAll(keepingCapacity: true)
        let levels = spectrum.levels(block)
        let now = Date().timeIntervalSinceReferenceDate
        guard now - lastSent >= 1.0 / 20 else { return }
        lastSent = now
        onLevels?(levels)
    }

    private static func defaultOutputUID() throws -> String {
        var device = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultSystemOutputDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &device) == noErr else {
            throw Failure("no output device")
        }
        var uid: Unmanaged<CFString>?
        size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        addr.mSelector = kAudioDevicePropertyDeviceUID
        guard AudioObjectGetPropertyData(device, &addr, 0, nil, &size, &uid) == noErr, let s = uid?.takeRetainedValue() else {
            throw Failure("no output device id")
        }
        return s as String
    }

    private func check(_ status: OSStatus, _ what: String) throws {
        guard status == noErr else { stop(); throw Failure("couldn't \(what) (\(status))") }
    }
}

/// "System Audio Recording": macOS doesn't prompt when a tap starts (it just hears silence), so ask through TCC,
/// as Widgets does. The prompt names the app that started this helper: Palette.
enum Permission {
    private typealias Preflight = @convention(c) (CFString, CFDictionary?) -> Int
    private typealias Request = @convention(c) (CFString, CFDictionary?, @escaping @convention(block) (Bool) -> Void) -> Void
    private static var service: CFString { "kTCCServiceAudioCapture" as CFString }
    private static var tcc: UnsafeMutableRawPointer? { dlopen("/System/Library/PrivateFrameworks/TCC.framework/TCC", RTLD_NOW) }

    /// 0 = allowed, 1 = denied, anything else = not asked yet.
    static func status() -> Int {
        guard let tcc, let f = dlsym(tcc, "TCCAccessPreflight") else { return -1 }
        return unsafeBitCast(f, to: Preflight.self)(service, nil)
    }

    static func request(_ done: @escaping (Bool) -> Void) {
        guard let tcc, let f = dlsym(tcc, "TCCAccessRequest") else { done(false); return }
        unsafeBitCast(f, to: Request.self)(service, nil) { ok in done(ok) }
    }
}

struct Failure: Error, CustomStringConvertible {
    let description: String
    init(_ s: String) { description = s }
}

// ---------------------------------------------------------------- main

func arg(_ name: String, _ fallback: String) -> String {
    let a = CommandLine.arguments
    if let i = a.firstIndex(of: name), i + 1 < a.count { return a[i + 1] }
    return fallback
}

func log(_ s: String) { FileHandle.standardError.write(Data("palette-beats: \(s)\n".utf8)) }

/// The state file: "playing" / "paused" / "none", then the player's bundle id on a second line
/// (owner, 2026-11-11: comparing the whole file never saw "playing", so nothing asked or listened).
func firstLine(_ text: String) -> String {
    String(text.split(separator: "\n", omittingEmptySubsequences: false).first ?? "")
}

/// One `--set` per bar, in one message.
func frame(_ heights: [Int]) -> [String] {
    heights.enumerated().flatMap { ["--set", "beat.\($0.offset + 1)", "background.height=\($0.element)"] }
}

// `palette-beats --send --add event x`: one command through the mach port (to check that route by hand).
if let i = CommandLine.arguments.firstIndex(of: "--send") {
    let ok = SketchyBar().send(Array(CommandLine.arguments[(i + 1)...]))
    print(ok ? "sent" : "SketchyBar isn't running")
    exit(ok ? 0 : 1)
}

if CommandLine.arguments.contains("--selftest") {
    let s = Spectrum(bands: 6, sampleRate: 48_000)
    var levels: [Double] = []
    for n in 0..<8 {   // a 1 kHz tone: the band holding 1 kHz should lead
        let block = (0..<1024).map { Float(sin(2 * Double.pi * 1000 * Double($0 + n * 1024) / 48_000)) }
        levels = s.levels(block)
    }
    let loudest = levels.firstIndex(of: levels.max()!)!
    let hz = s.ranges[loudest].map { Double($0) * 48_000 / 1024 }
    print("bands \(levels.map { String(format: "%.2f", $0) }.joined(separator: " ")); loudest \(loudest) covers \(Int(hz.first!))-\(Int(hz.last!)) Hz")
    print("message \(SketchyBar.bytes(frame([3, 7])).count) bytes: \(frame([3, 7]).joined(separator: " "))")
    let stateOK = firstLine("playing\ncom.spotify.client\n") == "playing" && firstLine("none\n") == "none"
    print(hz.contains { abs($0 - 1000) < 60 } && stateOK ? "ok" : "FAILED")
    exit(0)
}

let bars = max(1, min(Int(arg("--bars", "6")) ?? 6, 16))
let minH = Int(arg("--min", "2")) ?? 2
let maxH = max(minH + 1, Int(arg("--max", "18")) ?? 18)
let statePath = (arg("--state", "~/Library/Caches/palette/nowplaying.state") as NSString).expandingTildeInPath
let parent = getppid()

let bar = SketchyBar()
let tap = AudioTap(bands: bars)
var last: [Int] = []
var asked = false
var lastStatus = -99

func show(_ heights: [Int]) {
    guard heights != last else { return }
    if bar.send(frame(heights)) { last = heights }
}

tap.onLevels = { levels in
    let heights = levels.map { minH + Int(($0 * Double(maxH - minH)).rounded()) }
    show(heights)
}

/// Once a second: follow the feeder's state; leave when Palette's app (the parent) has gone.
func tick() {
    if getppid() != parent || getppid() == 1 { tap.stop(); exit(0) }
    let state = (try? String(contentsOfFile: statePath, encoding: .utf8)).map(firstLine)
    let playing = state == "playing"
    if playing, !tap.isRunning {
        let status = Permission.status()
        if status != lastStatus {
            log("permission: \(status == 0 ? "allowed" : status == 1 ? "denied" : "not asked yet (\(status))")")
            lastStatus = status
        }
        switch status {
        case 0:
            do { try tap.start(); log("listening") } catch { log("\(error)") }
        case 1:
            break   // denied: flat bars (System Settings → Privacy & Security → Screen & System Audio Recording)
        default:
            if !asked { asked = true; log("asking"); Permission.request { ok in log(ok ? "allowed" : "not allowed") } }
        }
    } else if !playing, tap.isRunning {
        tap.stop()
        log("stopped (\(state ?? "no state"))")
    }
    if !tap.isRunning { show(Array(repeating: minH, count: bars)) }
}

let timer = Timer(timeInterval: 1, repeats: true) { _ in tick() }
RunLoop.main.add(timer, forMode: .common)
signal(SIGTERM) { _ in exit(0) }
tick()
RunLoop.main.run()
