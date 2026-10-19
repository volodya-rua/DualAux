// Usage: mac list | start UID_A UID_B | stop | vol UID 0..1
// Untested on real hardware: written against the CoreAudio API docs.
import CoreAudio
import Foundation

let sys = AudioObjectID(kAudioObjectSystemObject)
func addr(_ s: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal,
          _ el: UInt32 = kAudioObjectPropertyElementMain) -> AudioObjectPropertyAddress {
  AudioObjectPropertyAddress(mSelector: s, mScope: scope, mElement: el)
}
func str(_ id: AudioObjectID, _ s: AudioObjectPropertySelector) -> String {
  var a = addr(s), v: CFString = "" as CFString, n = UInt32(MemoryLayout<CFString>.size)
  AudioObjectGetPropertyData(id, &a, 0, nil, &n, &v)
  return v as String
}
func devices() -> [AudioObjectID] {
  var a = addr(kAudioHardwarePropertyDevices), n = UInt32(0)
  AudioObjectGetPropertyDataSize(sys, &a, 0, nil, &n)
  var ids = [AudioObjectID](repeating: 0, count: Int(n) / MemoryLayout<AudioObjectID>.size)
  AudioObjectGetPropertyData(sys, &a, 0, nil, &n, &ids)
  return ids
}
func isOutput(_ id: AudioObjectID) -> Bool {
  var a = addr(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput), n = UInt32(0)
  AudioObjectGetPropertyDataSize(id, &a, 0, nil, &n)
  return n > 0
}
func find(_ uid: String) -> AudioObjectID? { devices().first { str($0, kAudioDevicePropertyDeviceUID) == uid } }
func setDefault(_ id: AudioObjectID) {
  var a = addr(kAudioHardwarePropertyDefaultOutputDevice), v = id
  AudioObjectSetPropertyData(sys, &a, 0, nil, UInt32(MemoryLayout<AudioObjectID>.size), &v)
}
func getDefault() -> AudioObjectID {
  var a = addr(kAudioHardwarePropertyDefaultOutputDevice), v = AudioObjectID(0), n = UInt32(MemoryLayout<AudioObjectID>.size)
  AudioObjectGetPropertyData(sys, &a, 0, nil, &n, &v)
  return v
}

let prevFile = NSTemporaryDirectory() + "dualaux.prev"
let args = CommandLine.arguments
switch args.count > 1 ? args[1] : "" {
case "list":
  let out = devices().filter(isOutput).map { ["id": str($0, kAudioDevicePropertyDeviceUID), "label": str($0, kAudioObjectPropertyName)] }
    .filter { $0["id"] != "dualaux" }
  print(String(data: try! JSONSerialization.data(withJSONObject: out), encoding: .utf8)!)
case "start":
  try? String(getDefault()).write(toFile: prevFile, atomically: true, encoding: .utf8)
  let desc: [String: Any] = [
    kAudioAggregateDeviceNameKey: "DualAux", kAudioAggregateDeviceUIDKey: "dualaux",
    kAudioAggregateDeviceMainSubDeviceKey: args[2], kAudioAggregateDeviceIsStackedKey: 1,
    kAudioAggregateDeviceSubDeviceListKey: args[2...].enumerated().map { (i, uid) -> [String: Any] in
      i == 0 ? [kAudioSubDeviceUIDKey: uid] : [kAudioSubDeviceUIDKey: uid, kAudioSubDeviceDriftCompensationKey: 1]
    },
  ]
  var agg = AudioObjectID(0)
  guard AudioHardwareCreateAggregateDevice(desc as CFDictionary, &agg) == 0 else { fputs("create failed\n", stderr); exit(1) }
  setDefault(agg)
case "stop":
  if let s = try? String(contentsOfFile: prevFile), let p = UInt32(s) { setDefault(p) }
  if let agg = find("dualaux") { AudioHardwareDestroyAggregateDevice(agg) }
case "vol":
  if let id = find(args[2]) {
    var v = Float32(args[3]) ?? 1
    for el in UInt32(1)...2 {  // left, right
      var a = addr(kAudioDevicePropertyVolumeScalar, kAudioObjectPropertyScopeOutput, el)
      AudioObjectSetPropertyData(id, &a, 0, nil, UInt32(MemoryLayout<Float32>.size), &v)
    }
  }
default: fputs("usage\n", stderr); exit(2)
}
