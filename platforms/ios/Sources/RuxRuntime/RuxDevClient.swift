#if DEBUG
import CRux
import Foundation

/// Hot reload client (debug builds): connects to the dev server of `rux run` and loads the
/// library builds it sends.
///
/// Protocol (one connection, line-based headers):
/// - server → app: `reload <size>\n` or `restart <size>\n`, then `<size>` bytes of the library;
/// - app → server: one report line, `ok …` or `err …`.
enum RuxDevClient {
    /// Environment variable carrying the server address (`host:port`), set by `rux run`.
    static let environmentKey = "RUX_DEV_SERVER"

    private static var started = false

    /// Starts the client once per process. Call on the main thread.
    static func connect(_ address: String) {
        guard !started else { return }
        started = true
        let thread = Thread { run(address) }
        thread.name = "rux-dev"
        thread.start()
    }

    private static func run(_ address: String) {
        defer { DispatchQueue.main.async { started = false } }
        let parts = address.split(separator: ":")
        guard parts.count == 2, let port = Int(parts[1]) else { return }

        var input: InputStream?
        var output: OutputStream?
        Stream.getStreamsToHost(withName: String(parts[0]), port: port, inputStream: &input, outputStream: &output)
        guard let input, let output else { return }
        input.open()
        output.open()
        defer {
            input.close()
            output.close()
        }

        // Builds are written next to each other: a loaded library can't be replaced in place.
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("rux-hot")
        try? FileManager.default.removeItem(at: dir)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

        var build = 0
        while let header = readLine(input) {
            let fields = header.split(separator: " ")
            guard fields.count == 2, let size = Int(fields[1]), let bytes = read(input, size) else { break }
            build += 1
            let file = dir.appendingPathComponent("build-\(build).dylib")
            guard (try? bytes.write(to: file)) != nil else { break }

            // Reload on the main thread, where the app runs.
            let keepState = fields[0] == "reload"
            let report = DispatchQueue.main.sync {
                String(cString: rux_reload(file.path, keepState))
            }
            NSLog("[rux] hot reload: %@", report)
            write(output, report.replacingOccurrences(of: "\n", with: " ") + "\n")
        }
    }

    private static func readLine(_ input: InputStream) -> String? {
        var line = Data()
        var byte: UInt8 = 0
        while input.read(&byte, maxLength: 1) == 1 {
            if byte == UInt8(ascii: "\n") { return String(decoding: line, as: UTF8.self) }
            line.append(byte)
        }
        return nil
    }

    private static func read(_ input: InputStream, _ size: Int) -> Data? {
        var data = Data(count: size)
        var offset = 0
        while offset < size {
            let n = data.withUnsafeMutableBytes { buf in
                input.read(buf.baseAddress!.assumingMemoryBound(to: UInt8.self) + offset, maxLength: size - offset)
            }
            if n <= 0 { return nil }
            offset += n
        }
        return data
    }

    private static func write(_ output: OutputStream, _ text: String) {
        let bytes = Array(text.utf8)
        var offset = 0
        while offset < bytes.count {
            let n = bytes[offset...].withUnsafeBufferPointer { output.write($0.baseAddress!, maxLength: $0.count) }
            if n <= 0 { return }
            offset += n
        }
    }
}
#endif
