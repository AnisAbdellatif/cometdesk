// Usage: swift tools/gen_h264_fixtures.swift crates/glkvm-media/tests/data
// Generates tiny Annex B H.264 test fixtures (Constrained Baseline) with VideoToolbox.
import Foundation
import VideoToolbox

let outDir = CommandLine.arguments[1]
let w = 64, h = 64
var frames: [Data] = []

var session: VTCompressionSession?
let st = VTCompressionSessionCreate(allocator: nil, width: Int32(w), height: Int32(h),
    codecType: kCMVideoCodecType_H264, encoderSpecification: nil, imageBufferAttributes: nil,
    compressedDataAllocator: nil, outputCallback: nil, refcon: nil, compressionSessionOut: &session)
precondition(st == 0, "create \(st)")
let s = session!
VTSessionSetProperty(s, key: kVTCompressionPropertyKey_RealTime, value: kCFBooleanTrue)
VTSessionSetProperty(s, key: kVTCompressionPropertyKey_ProfileLevel, value: kVTProfileLevel_H264_ConstrainedBaseline_AutoLevel)
VTSessionSetProperty(s, key: kVTCompressionPropertyKey_AllowFrameReordering, value: kCFBooleanFalse)

func annexB(_ sb: CMSampleBuffer, includeParams: Bool) -> Data {
    var out = Data()
    let sc: [UInt8] = [0, 0, 0, 1]
    if includeParams, let fd = CMSampleBufferGetFormatDescription(sb) {
        var count = 0
        CMVideoFormatDescriptionGetH264ParameterSetAtIndex(fd, parameterSetIndex: 0, parameterSetPointerOut: nil, parameterSetSizeOut: nil, parameterSetCountOut: &count, nalUnitHeaderLengthOut: nil)
        for i in 0..<count {
            var p: UnsafePointer<UInt8>?; var n = 0
            CMVideoFormatDescriptionGetH264ParameterSetAtIndex(fd, parameterSetIndex: i, parameterSetPointerOut: &p, parameterSetSizeOut: &n, parameterSetCountOut: nil, nalUnitHeaderLengthOut: nil)
            out.append(contentsOf: sc); out.append(p!, count: n)
        }
    }
    let bb = CMSampleBufferGetDataBuffer(sb)!
    var len = 0; var ptr: UnsafeMutablePointer<CChar>?
    CMBlockBufferGetDataPointer(bb, atOffset: 0, lengthAtOffsetOut: nil, totalLengthOut: &len, dataPointerOut: &ptr)
    let raw = UnsafeRawPointer(ptr!)
    var off = 0
    while off + 4 <= len {
        let n = Int(UInt32(bigEndian: raw.loadUnaligned(fromByteOffset: off, as: UInt32.self)))
        out.append(contentsOf: sc)
        out.append(raw.advanced(by: off + 4).assumingMemoryBound(to: UInt8.self), count: n)
        off += 4 + n
    }
    return out
}

for i in 0..<2 {
    var pb: CVPixelBuffer?
    CVPixelBufferCreate(nil, w, h, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, nil, &pb)
    CVPixelBufferLockBaseAddress(pb!, [])
    for plane in 0..<2 {
        let base = CVPixelBufferGetBaseAddressOfPlane(pb!, plane)!
        memset(base, plane == 0 ? Int32(64 + i * 64) : 128, CVPixelBufferGetBytesPerRowOfPlane(pb!, plane) * CVPixelBufferGetHeightOfPlane(pb!, plane))
    }
    CVPixelBufferUnlockBaseAddress(pb!, [])
    let props: CFDictionary? = i == 0 ? [kVTEncodeFrameOptionKey_ForceKeyFrame: true] as CFDictionary : nil
    VTCompressionSessionEncodeFrame(s, imageBuffer: pb!, presentationTimeStamp: CMTime(value: Int64(i), timescale: 60),
        duration: .invalid, frameProperties: props, infoFlagsOut: nil) { status, _, sb in
        precondition(status == 0 && sb != nil)
        frames.append(annexB(sb!, includeParams: i == 0))
    }
}
VTCompressionSessionCompleteFrames(s, untilPresentationTimeStamp: .invalid)
precondition(frames.count == 2, "got \(frames.count) frames")
try! frames[0].write(to: URL(fileURLWithPath: outDir + "/gray64.h264"))
try! frames[1].write(to: URL(fileURLWithPath: outDir + "/gray64_p.h264"))
print("key \(frames[0].count) bytes, delta \(frames[1].count) bytes")
