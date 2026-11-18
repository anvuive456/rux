import Foundation

/// Binary wire format of native API calls — must match `rux_native::codec` (Rust):
/// little-endian; strings and lists are prefixed by a u32 length; optionals by a 0/1 byte;
/// enums are their i32 variant index; structs are their fields in order.
///
/// Used by the code `rux build` generates from `#[rux::host_api]` declarations.
public final class RuxWriter {
    public private(set) var data = Data()

    public init() {}

    public func bool(_ v: Bool) { data.append(v ? 1 : 0) }
    public func u8(_ v: UInt8) { data.append(v) }
    public func int32(_ v: Int32) { append(v.littleEndian) }
    public func int64(_ v: Int64) { append(v.littleEndian) }
    public func double(_ v: Double) { append(v.bitPattern.littleEndian) }

    public func string(_ v: String) {
        let bytes = Array(v.utf8)
        append(UInt32(bytes.count).littleEndian)
        data.append(contentsOf: bytes)
    }

    public func optional<T>(_ v: T?, _ write: (T) -> Void) {
        if let v {
            u8(1)
            write(v)
        } else {
            u8(0)
        }
    }

    public func list<T>(_ v: [T], _ write: (T) -> Void) {
        append(UInt32(v.count).littleEndian)
        v.forEach(write)
    }

    private func append<T: FixedWidthInteger>(_ v: T) {
        withUnsafeBytes(of: v) { data.append(contentsOf: $0) }
    }
}

public final class RuxReader {
    private let bytes: [UInt8]
    private var pos = 0

    public init(_ data: Data) { bytes = Array(data) }

    public func bool() throws -> Bool {
        switch try take(1)[0] {
        case 0: return false
        case 1: return true
        case let b: throw RuxError(code: "decode", message: "invalid bool \(b)")
        }
    }

    public func int32() throws -> Int32 { Int32(littleEndian: try fixed()) }
    public func int64() throws -> Int64 { Int64(littleEndian: try fixed()) }
    public func double() throws -> Double { Double(bitPattern: UInt64(littleEndian: try fixed())) }

    public func string() throws -> String {
        let count = Int(UInt32(littleEndian: try fixed()))
        return String(decoding: try take(count), as: UTF8.self)
    }

    public func optional<T>(_ read: () throws -> T) throws -> T? {
        switch try take(1)[0] {
        case 0: return nil
        case 1: return try read()
        case let tag: throw RuxError(code: "decode", message: "invalid optional tag \(tag)")
        }
    }

    public func list<T>(_ read: () throws -> T) throws -> [T] {
        let count = Int(UInt32(littleEndian: try fixed()))
        return try (0..<count).map { _ in try read() }
    }

    private func take(_ n: Int) throws -> ArraySlice<UInt8> {
        guard pos + n <= bytes.count else {
            throw RuxError(code: "decode", message: "unexpected end of data")
        }
        defer { pos += n }
        return bytes[pos..<pos + n]
    }

    private func fixed<T: FixedWidthInteger>() throws -> T {
        let slice = try take(MemoryLayout<T>.size)
        return slice.withUnsafeBytes { $0.loadUnaligned(as: T.self) }
    }
}

/// An error returned to Rust as `NativeError { code, message }`.
public struct RuxError: Error {
    public let code: String
    public let message: String

    public init(code: String, message: String) {
        self.code = code
        self.message = message
    }
}
