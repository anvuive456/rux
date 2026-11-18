//! Swift for native APIs: structs / enums with their codec, a protocol per API with a
//! `<Api>Setup.setUp(api:)` dispatcher, and the `@objc(RuxRegistrant)` class `RuxChannels`
//! runs at startup.

use std::fmt::Write;

use super::naming::camel;
use super::schema::{Api, Method, Schema, Ty, TypeDef};
use super::{HEADER, NativePackage};

/// The generated file for one crate.
pub fn file(crate_name: &str, schema: &Schema) -> String {
    let mut out = format!("// {HEADER}\n\nimport Foundation\nimport RuxRuntime\n");
    for t in &schema.types {
        out.push('\n');
        type_def(&mut out, t);
    }
    for api in &schema.apis {
        out.push('\n');
        api_def(&mut out, crate_name, api);
    }
    out
}

/// `RuxRegistrant`: registers every API implementation (constructed with `init()`).
pub fn registrant(packages: &[NativePackage]) -> String {
    let mut out = format!(
        "// {HEADER}\n\nimport Foundation\n\n@objc(RuxRegistrant)\nfinal class RuxRegistrant: NSObject {{\n    \
         @objc static func register() {{\n"
    );
    for (_, api, imp) in super::implementations(packages, |i| i.ios.as_deref()) {
        let _ = writeln!(out, "        {api}Setup.setUp(api: {imp}())");
    }
    out.push_str("    }\n}\n");
    out
}

fn type_def(out: &mut String, t: &TypeDef) {
    match t {
        TypeDef::Struct { name, fields } => {
            let _ = writeln!(out, "struct {name} {{");
            for (f, ty) in fields {
                let _ = writeln!(out, "    var {}: {}", camel(f), swift(ty));
            }
            let _ = writeln!(
                out,
                "}}\n\nextension {name} {{\n    func encode(_ w: RuxWriter) {{"
            );
            for (f, ty) in fields {
                let _ = writeln!(out, "        {}", encode(ty, &camel(f), 0));
            }
            let _ = writeln!(
                out,
                "    }}\n\n    static func decode(_ r: RuxReader) throws -> {name} {{\n        {name}("
            );
            let args: Vec<_> = fields
                .iter()
                .map(|(f, ty)| format!("            {}: {}", camel(f), decode(ty)))
                .collect();
            let _ = writeln!(out, "{}\n        )\n    }}\n}}", args.join(",\n"));
        }
        TypeDef::Enum { name, variants } => {
            let _ = writeln!(out, "enum {name}: Int32 {{");
            for (i, v) in variants.iter().enumerate() {
                let _ = writeln!(out, "    case {} = {i}", camel(v));
            }
            let _ = writeln!(
                out,
                "\n    func encode(_ w: RuxWriter) {{ w.int32(rawValue) }}\n\n    \
                 static func decode(_ r: RuxReader) throws -> {name} {{\n        \
                 let raw = try r.int32()\n        guard let v = {name}(rawValue: raw) else {{\n            \
                 throw RuxError(code: \"decode\", message: \"invalid {name} \\(raw)\")\n        }}\n        \
                 return v\n    }}\n}}"
            );
        }
    }
}

fn api_def(out: &mut String, crate_name: &str, api: &Api) {
    let name = &api.name;
    let _ = writeln!(
        out,
        "/// Implement and register in `[package.metadata.rux.native]` (initializer: `init()`).\n\
         protocol {name} {{"
    );
    for m in &api.methods {
        let _ = writeln!(out, "    {}", signature(m));
    }
    let _ = writeln!(
        out,
        "}}\n\nenum {name}Setup {{\n    static let name = \"{crate_name}.{name}\"\n\n    \
         /// Registers (or with `nil`, removes) the implementation.\n    \
         static func setUp(api: {name}?) {{\n        \
         guard let api else {{ return RuxChannels.register(name, nil) }}\n        \
         RuxChannels.register(name) {{ method, r, reply in\n            switch method {{"
    );
    for (i, m) in api.methods.iter().enumerate() {
        let decode_args: String = m
            .args
            .iter()
            .map(|(a, ty)| format!("let {} = {}; ", camel(a), decode(ty)))
            .collect();
        let labels: Vec<_> = m
            .args
            .iter()
            .map(|(a, _)| format!("{0}: {0}", camel(a)))
            .collect();
        let body = if m.is_async {
            let mut labels = labels;
            labels.push(format!(
                "completion: {{ result in reply.complete(result) {{ w, v in {} }} }}",
                encode_or_nothing(&m.ret, "v")
            ));
            format!("{decode_args}api.{}({})", camel(&m.name), labels.join(", "))
        } else {
            format!(
                "{decode_args}let v = try api.{}({}); reply.success {{ w in {} }}",
                camel(&m.name),
                labels.join(", "),
                encode_or_nothing(&m.ret, "v")
            )
        };
        let _ = writeln!(out, "            case {i}: {body}");
    }
    out.push_str(
        "            default: reply.notImplemented(method)\n            }\n        }\n    }\n}\n",
    );
}

fn signature(m: &Method) -> String {
    let mut params: Vec<_> = m
        .args
        .iter()
        .map(|(a, ty)| format!("{}: {}", camel(a), swift(ty)))
        .collect();
    if m.is_async {
        params.push(format!(
            "completion: @escaping (Result<{}, Error>) -> Void",
            swift(&m.ret)
        ));
        format!("func {}({})", camel(&m.name), params.join(", "))
    } else {
        format!(
            "func {}({}) throws -> {}",
            camel(&m.name),
            params.join(", "),
            swift(&m.ret)
        )
    }
}

fn swift(ty: &Ty) -> String {
    match ty {
        Ty::Bool => "Bool".into(),
        Ty::I32 => "Int32".into(),
        Ty::I64 => "Int64".into(),
        Ty::F64 => "Double".into(),
        Ty::String => "String".into(),
        Ty::Unit => "Void".into(),
        Ty::Option(t) => format!("{}?", swift(t)),
        Ty::List(t) => format!("[{}]", swift(t)),
        Ty::Named(n) => n.clone(),
    }
}

fn encode_or_nothing(ty: &Ty, value: &str) -> String {
    if *ty == Ty::Unit {
        "_ = (w, v)".into()
    } else {
        encode(ty, value, 1)
    }
}

/// Statement writing `value` (of `ty`) to `w`.
fn encode(ty: &Ty, value: &str, depth: usize) -> String {
    let v = format!("x{depth}");
    match ty {
        Ty::Bool => format!("w.bool({value})"),
        Ty::I32 => format!("w.int32({value})"),
        Ty::I64 => format!("w.int64({value})"),
        Ty::F64 => format!("w.double({value})"),
        Ty::String => format!("w.string({value})"),
        Ty::Unit => String::new(),
        Ty::Option(t) => format!(
            "w.optional({value}) {{ {v} in {} }}",
            encode(t, &v, depth + 1)
        ),
        Ty::List(t) => format!("w.list({value}) {{ {v} in {} }}", encode(t, &v, depth + 1)),
        Ty::Named(_) => format!("{value}.encode(w)"),
    }
}

/// Expression reading a `ty` from `r`.
fn decode(ty: &Ty) -> String {
    match ty {
        Ty::Bool => "try r.bool()".into(),
        Ty::I32 => "try r.int32()".into(),
        Ty::I64 => "try r.int64()".into(),
        Ty::F64 => "try r.double()".into(),
        Ty::String => "try r.string()".into(),
        Ty::Unit => "()".into(),
        Ty::Option(t) => format!("try r.optional {{ {} }}", decode(t)),
        Ty::List(t) => format!("try r.list {{ {} }}", decode(t)),
        Ty::Named(n) => format!("try {n}.decode(r)"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::schema::parse;
    use super::file;

    #[test]
    fn generates_types_protocol_and_dispatcher() {
        let schema = parse(
            r#"
            #[derive(NativeType)] pub struct Info { pub user_name: String, pub level: Option<i32> }
            #[derive(NativeType)] pub enum Kind { Phone, SmartWatch }
            #[host_api] pub trait DemoApi {
                fn info(&self, verbose: bool) -> Result<Info, E>;
                #[async_call] fn save(&self) -> Result<(), E>;
            }"#,
        )
        .unwrap();
        let swift = file("my-plugin", &schema);
        for expected in [
            "struct Info {\n    var userName: String\n    var level: Int32?\n}",
            "w.optional(level) { x0 in w.int32(x0) }",
            "level: try r.optional { try r.int32() }",
            "enum Kind: Int32 {\n    case phone = 0\n    case smartWatch = 1",
            "    func info(verbose: Bool) throws -> Info\n",
            "    func save(completion: @escaping (Result<Void, Error>) -> Void)\n",
            "static let name = \"my-plugin.DemoApi\"",
            "case 0: let verbose = try r.bool(); let v = try api.info(verbose: verbose); reply.success { w in v.encode(w) }",
            "case 1: api.save(completion: { result in reply.complete(result) { w, v in _ = (w, v) } })",
        ] {
            assert!(
                swift.contains(expected),
                "missing:\n{expected}\n--- in ---\n{swift}"
            );
        }
    }
}
