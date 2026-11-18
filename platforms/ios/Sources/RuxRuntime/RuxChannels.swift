import CRux
import Foundation

/// Native API registry (`#[rux::host_api]`): Rust calls in (through `RuxHostVTable.invoke_native`)
/// and the implementation registered under the API name (`"<crate>.<Trait>"`) runs.
///
/// Implementations are registered by the generated `RuxRegistrant` (from
/// `[package.metadata.rux.native]` of the app and its plugins), which `RuxHost.start()` runs;
/// or by hand with the generated `<Api>Setup.setUp(api:)`.
public enum RuxChannels {
    /// Decodes a call's arguments, runs the implementation, answers through `reply`.
    public typealias Handler = (_ method: Int32, _ args: RuxReader, _ reply: RuxReply) throws -> Void

    private static var handlers: [String: Handler] = [:]

    /// Registers (or with `nil`, removes) the implementation of `api`. Main thread.
    public static func register(_ api: String, _ handler: Handler?) {
        handlers[api] = handler
    }

    /// Runs the generated registrant (`@objc(RuxRegistrant)`), if the app has native APIs.
    static func registerGenerated() {
        guard let registrant = NSClassFromString("RuxRegistrant") as AnyObject? else { return }
        _ = registrant.perform(NSSelectorFromString("register"))
    }

    /// Called from Rust on the main thread.
    static func invoke(_ api: String, _ method: Int32, _ callId: UInt64, _ args: Data) {
        let reply = RuxReply(callId)
        guard let handler = handlers[api] else {
            return reply.error(RuxError(code: "not_implemented", message: "no implementation registered for \(api)"))
        }
        do {
            try handler(method, RuxReader(args), reply)
        } catch {
            reply.error(error)
        }
    }
}

/// Answers one native API call — exactly once, from any thread.
public final class RuxReply {
    private let callId: UInt64

    init(_ callId: UInt64) { self.callId = callId }

    /// Encodes a successful result with `write`; a thrown error becomes an error reply.
    public func success(_ write: (RuxWriter) throws -> Void) {
        let w = RuxWriter()
        w.u8(0)
        do {
            try write(w)
        } catch {
            return self.error(error)
        }
        send(w.data)
    }

    /// Completes an async call from a `Result`.
    public func complete<T>(_ result: Result<T, Error>, _ write: (RuxWriter, T) -> Void) {
        switch result {
        case let .success(v): success { w in write(w, v) }
        case let .failure(e): error(e)
        }
    }

    public func error(_ error: Error) {
        let w = RuxWriter()
        w.u8(1)
        if let e = error as? RuxError {
            w.string(e.code)
            w.string(e.message)
        } else {
            w.string(String(describing: type(of: error)))
            w.string(error.localizedDescription)
        }
        send(w.data)
    }

    public func notImplemented(_ method: Int32) {
        error(RuxError(code: "not_implemented", message: "unknown method #\(method)"))
    }

    private func send(_ payload: Data) {
        let callId = callId
        let deliver = {
            payload.withUnsafeBytes { buf in
                rux_native_reply(callId, buf.bindMemory(to: UInt8.self).baseAddress, buf.count)
            }
        }
        if Thread.isMainThread {
            deliver()
        } else {
            DispatchQueue.main.async(execute: deliver)
        }
    }
}
