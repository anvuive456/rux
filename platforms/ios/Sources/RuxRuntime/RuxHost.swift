import CRux
import UIKit

/// iOS implementation of the mutation protocol (`rux_core::Mutation`).
///
/// Rust calls in through `RuxHostVTable` (see `rux.h`), always on the main thread.
/// Use from the main thread only; keep the instance alive for the app's lifetime (ctx is an unretained pointer).
public final class RuxHost {
    private var nodes: [UInt64: UIView] = [:]
    private var actions: [UInt64: [String: UIAction]] = [:]
    private weak var container: UIView?
    /// The view shown in the container. Kept as a view (not an id): on hot reload the old tree is
    /// released before the new root, which may reuse the old root's id, is set.
    private var rootView: UIView?

    /// true while dispatching / applying mutations — suppresses synchronously fired events.
    private var applying = false

    public init(container: UIView) {
        self.container = container
    }

    public func start() {
        let ctx = Unmanaged.passUnretained(self).toOpaque()
        let vtable = RuxHostVTable(
            ctx: ctx,
            create_node: { ctx, id, kind in RuxHost.from(ctx).createNode(id, String(cString: kind)) },
            set_prop: { ctx, id, name, value in
                RuxHost.from(ctx).setProp(id, String(cString: name), String(cString: value))
            },
            remove_prop: { ctx, id, name in RuxHost.from(ctx).removeProp(id, String(cString: name)) },
            insert_child: { ctx, parent, child, index in
                RuxHost.from(ctx).insertChild(parent, child, Int(index))
            },
            remove_child: { ctx, parent, child in RuxHost.from(ctx).removeChild(parent, child) },
            release_node: { ctx, id in RuxHost.from(ctx).releaseNode(id) },
            set_root: { ctx, id in RuxHost.from(ctx).setRoot(id) },
            listen: { ctx, id, event in RuxHost.from(ctx).listen(id, String(cString: event)) },
            unlisten: { ctx, id, event in RuxHost.from(ctx).unlisten(id, String(cString: event)) }
        )
        rux_start(vtable)
        #if DEBUG
        // `rux run` passes its dev server address to debug builds (hot reload).
        if let address = ProcessInfo.processInfo.environment[RuxDevClient.environmentKey] {
            RuxDevClient.connect(address)
        }
        #endif
    }

    public func stop() {
        rux_stop()
    }

    private static func from(_ ctx: UnsafeMutableRawPointer) -> RuxHost {
        Unmanaged<RuxHost>.fromOpaque(ctx).takeUnretainedValue()
    }

    // MARK: - mutation protocol

    func createNode(_ id: UInt64, _ kind: String) {
        let view: UIView
        switch kind {
        case "column":
            let stack = UIStackView()
            stack.axis = .vertical
            stack.alignment = .fill
            view = stack
        case "row":
            // Children keep their natural width, packed at the start (like Android's
            // WRAP_CONTENT): a flexible spacer after them takes the remaining width.
            let stack = UIStackView()
            stack.axis = .horizontal
            stack.alignment = .center
            stack.addArrangedSubview(RowSpacer())
            view = stack
        case "scroll":
            view = UIScrollView()
        case "text":
            let label = UILabel()
            label.numberOfLines = 0
            view = label
        case "button":
            view = UIButton(type: .system)
        case "text_input":
            let field = UITextField()
            field.borderStyle = .roundedRect
            view = field
        default:
            view = UIView()
        }
        nodes[id] = view
    }

    func setProp(_ id: UInt64, _ name: String, _ value: String) {
        mutate(id) { v in self.apply(v, name, value) }
    }

    func removeProp(_ id: UInt64, _ name: String) {
        mutate(id) { v in
            switch name {
            case "text", "value": self.apply(v, name, "")
            case "placeholder": (v as? UITextField)?.placeholder = nil
            case "background": v.backgroundColor = nil
            case "padding": self.apply(v, name, "0")
            case "spacing": (v as? UIStackView)?.spacing = 0
            case "enabled": self.apply(v, name, "true")
            default: break
            }
        }
    }

    func insertChild(_ parent: UInt64, _ child: UInt64, _ index: Int) {
        guard let p = nodes[parent], let c = nodes[child] else { return }
        c.removeFromSuperview()
        switch p {
        case let stack as UIStackView:
            // Keep a row's spacer last.
            let count = stack.arrangedSubviews.filter { !($0 is RowSpacer) }.count
            stack.insertArrangedSubview(c, at: min(index, count))
        case let scroll as UIScrollView:
            c.translatesAutoresizingMaskIntoConstraints = false
            scroll.addSubview(c)
            NSLayoutConstraint.activate([
                c.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
                c.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
                c.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
                c.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
                c.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor),
            ])
        default:
            p.insertSubview(c, at: min(index, p.subviews.count))
        }
    }

    func removeChild(_ parent: UInt64, _ child: UInt64) {
        guard let c = nodes[child], c.superview === nodes[parent] else { return }
        c.removeFromSuperview()
    }

    func releaseNode(_ id: UInt64) {
        if let control = nodes[id] as? UIControl, let acts = actions[id] {
            for (event, action) in acts { control.removeAction(action, for: Self.controlEvent(event)) }
        }
        actions[id] = nil
        nodes[id] = nil
    }

    func setRoot(_ id: UInt64) {
        guard let container, let root = nodes[id] else { return }
        if let old = rootView, old !== root { old.removeFromSuperview() }
        rootView = root
        root.removeFromSuperview()
        root.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(root)
        let guide = container.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            root.topAnchor.constraint(equalTo: guide.topAnchor),
            root.leadingAnchor.constraint(equalTo: guide.leadingAnchor),
            root.trailingAnchor.constraint(equalTo: guide.trailingAnchor),
            root.bottomAnchor.constraint(lessThanOrEqualTo: guide.bottomAnchor),
        ])
    }

    func listen(_ id: UInt64, _ event: String) {
        guard let control = nodes[id] as? UIControl else { return }
        let action = UIAction { [weak self] action in
            let value = (action.sender as? UITextField)?.text
            self?.dispatch(id, event, value)
        }
        if let old = actions[id]?[event] { control.removeAction(old, for: Self.controlEvent(event)) }
        control.addAction(action, for: Self.controlEvent(event))
        actions[id, default: [:]][event] = action
    }

    func unlisten(_ id: UInt64, _ event: String) {
        guard let control = nodes[id] as? UIControl,
              let action = actions[id]?.removeValue(forKey: event) else { return }
        control.removeAction(action, for: Self.controlEvent(event))
    }

    // MARK: - helpers

    private func dispatch(_ id: UInt64, _ event: String, _ value: String?) {
        guard !applying else { return }
        applying = true
        defer { applying = false }
        if let value {
            value.withCString { rux_dispatch(id, event, $0) }
        } else {
            rux_dispatch(id, event, nil)
        }
    }

    private func mutate(_ id: UInt64, _ block: (UIView) -> Void) {
        guard let v = nodes[id] else { return }
        let prev = applying
        applying = true
        defer { applying = prev }
        block(v)
    }

    private func apply(_ v: UIView, _ name: String, _ value: String) {
        switch name {
        case "text":
            switch v {
            case let l as UILabel: l.text = value
            case let b as UIButton: b.setTitle(value, for: .normal)
            case let f as UITextField: f.text = value
            default: break
            }
        case "value":
            if let f = v as? UITextField, f.text != value { f.text = value }
        case "placeholder":
            (v as? UITextField)?.placeholder = value
        case "font_size":
            let font = UIFont.systemFont(ofSize: CGFloat(Double(value) ?? 17))
            switch v {
            case let l as UILabel: l.font = font
            case let b as UIButton: b.titleLabel?.font = font
            case let f as UITextField: f.font = font
            default: break
            }
        case "color":
            let color = Self.parseColor(value)
            switch v {
            case let l as UILabel: l.textColor = color
            case let b as UIButton: b.setTitleColor(color, for: .normal)
            case let f as UITextField: f.textColor = color
            default: break
            }
        case "background":
            v.backgroundColor = Self.parseColor(value)
        case "padding":
            let p = CGFloat(Double(value) ?? 0)
            v.layoutMargins = UIEdgeInsets(top: p, left: p, bottom: p, right: p)
            (v as? UIStackView)?.isLayoutMarginsRelativeArrangement = true
        case "spacing":
            (v as? UIStackView)?.spacing = CGFloat(Double(value) ?? 0)
        case "enabled":
            (v as? UIControl)?.isEnabled = value == "true"
        default:
            break
        }
    }

    private static func controlEvent(_ event: String) -> UIControl.Event {
        switch event {
        case "change": return .editingChanged
        default: return .touchUpInside
        }
    }

    /// `#AARRGGBB` → UIColor.
    private static func parseColor(_ value: String) -> UIColor {
        guard let argb = UInt32(value.dropFirst(), radix: 16) else { return .magenta }
        return UIColor(
            red: CGFloat((argb >> 16) & 0xFF) / 255,
            green: CGFloat((argb >> 8) & 0xFF) / 255,
            blue: CGFloat(argb & 0xFF) / 255,
            alpha: CGFloat((argb >> 24) & 0xFF) / 255
        )
    }
}

/// Trailing filler of a `row` stack: absorbs the extra width instead of the children.
private final class RowSpacer: UIView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        setContentHuggingPriority(.fittingSizeLevel, for: .horizontal)
        setContentCompressionResistancePriority(.fittingSizeLevel, for: .horizontal)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }
}
