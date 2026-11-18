//! Kotlin for native APIs: data classes / enums with their codec, an interface per API with a
//! `setUp(impl)` dispatcher, and the registrant `RuxChannels` runs at startup.

use std::fmt::Write;

use super::naming::{camel, upper_snake};
use super::schema::{Api, Method, Schema, Ty, TypeDef};
use super::{HEADER, NativePackage};

/// Kotlin package of a crate's generated code: `dev.rux.generated.<crate_name>`.
pub fn package_of(crate_name: &str) -> String {
    format!("dev.rux.generated.{}", crate_name.replace('-', "_"))
}

/// The generated file for one crate.
pub fn file(crate_name: &str, schema: &Schema) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "// {HEADER}");
    out.push_str("@file:Suppress(\"unused\", \"RedundantSuppression\")\n\n");
    let _ = writeln!(out, "package {}\n", package_of(crate_name));
    out.push_str("import dev.rux.runtime.RuxChannels\n");
    out.push_str("import dev.rux.runtime.RuxError\n");
    out.push_str("import dev.rux.runtime.RuxReader\n");
    out.push_str("import dev.rux.runtime.RuxWriter\n");
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

/// `dev.rux.generated.RuxRegistrant`: registers every API implementation (constructed with
/// the `Context`, i.e. the Activity).
pub fn registrant(packages: &[NativePackage]) -> String {
    let mut out = format!(
        "// {HEADER}\n\npackage dev.rux.generated\n\nimport android.content.Context\n\n\
         object RuxRegistrant {{\n    @JvmStatic\n    fun register(context: Context) {{\n"
    );
    for (pkg, api, imp) in super::implementations(packages, |i| i.android.as_deref()) {
        let _ = writeln!(
            out,
            "        {}.{api}.setUp({imp}(context))",
            package_of(&pkg.name)
        );
    }
    out.push_str("    }\n}\n");
    out
}

fn type_def(out: &mut String, t: &TypeDef) {
    match t {
        TypeDef::Struct { name, fields } => {
            let _ = writeln!(out, "data class {name}(");
            for (f, ty) in fields {
                let _ = writeln!(out, "    val {}: {},", camel(f), kt(ty));
            }
            out.push_str(") {\n    fun encode(w: RuxWriter) {\n");
            for (f, ty) in fields {
                let _ = writeln!(out, "        {}", encode(ty, &camel(f), 0));
            }
            let _ = writeln!(
                out,
                "    }}\n\n    companion object {{\n        fun decode(r: RuxReader): {name} = {name}("
            );
            for (f, ty) in fields {
                let _ = writeln!(out, "            {} = {},", camel(f), decode(ty));
            }
            out.push_str("        )\n    }\n}\n");
        }
        TypeDef::Enum { name, variants } => {
            let cases: Vec<_> = variants.iter().map(|v| upper_snake(v)).collect();
            let _ = writeln!(
                out,
                "enum class {name} {{\n    {};\n",
                cases.join(",\n    ")
            );
            out.push_str("    fun encode(w: RuxWriter) = w.int(ordinal)\n\n");
            let _ = writeln!(
                out,
                "    companion object {{\n        fun decode(r: RuxReader): {name} {{\n            \
                 val i = r.int()\n            return entries.getOrNull(i) \
                 ?: throw RuxError(\"decode\", \"invalid {name} $i\")\n        }}\n    }}\n}}"
            );
        }
    }
}

fn api_def(out: &mut String, crate_name: &str, api: &Api) {
    let name = &api.name;
    let _ = writeln!(
        out,
        "/** Implement and register in `[package.metadata.rux.native]` (constructor: `(Context)`). */\n\
         interface {name} {{"
    );
    for m in &api.methods {
        let _ = writeln!(out, "    {}", signature(m));
    }
    let _ = writeln!(
        out,
        "\n    companion object {{\n        const val NAME = \"{crate_name}.{name}\"\n\n        \
         /** Registers (or with `null`, removes) the implementation. */\n        \
         fun setUp(api: {name}?) {{\n            \
         RuxChannels.register(NAME, api?.let {{ impl ->\n                \
         RuxChannels.Handler {{ method, r, reply ->\n                    when (method) {{"
    );
    for (i, m) in api.methods.iter().enumerate() {
        let decode_args: String = m
            .args
            .iter()
            .map(|(a, ty)| format!("val {} = {}; ", camel(a), decode(ty)))
            .collect();
        let call_args: Vec<_> = m.args.iter().map(|(a, _)| camel(a)).collect();
        let call = format!("impl.{}({})", camel(&m.name), call_args.join(", "));
        let body = if m.is_async {
            let sep = if call_args.is_empty() { "" } else { ", " };
            let call = format!(
                "impl.{}({}{sep}{{ result -> reply.complete(result) {{ w, v -> {} }} }})",
                camel(&m.name),
                call_args.join(", "),
                encode_or_nothing(&m.ret, "v")
            );
            format!("{{ {decode_args}{call} }}")
        } else {
            format!(
                "{{ {decode_args}val v = {call}; reply.success {{ w -> {} }} }}",
                encode_or_nothing(&m.ret, "v")
            )
        };
        let _ = writeln!(out, "                        {i} -> {body}");
    }
    out.push_str(
        "                        else -> reply.notImplemented(method)\n                    }\n                \
         }\n            })\n        }\n    }\n}\n",
    );
}

fn signature(m: &Method) -> String {
    let mut params: Vec<_> = m
        .args
        .iter()
        .map(|(a, ty)| format!("{}: {}", camel(a), kt(ty)))
        .collect();
    if m.is_async {
        params.push(format!("callback: (Result<{}>) -> Unit", kt(&m.ret)));
        format!("fun {}({})", camel(&m.name), params.join(", "))
    } else {
        format!(
            "fun {}({}): {}",
            camel(&m.name),
            params.join(", "),
            kt(&m.ret)
        )
    }
}

fn kt(ty: &Ty) -> String {
    match ty {
        Ty::Bool => "Boolean".into(),
        Ty::I32 => "Int".into(),
        Ty::I64 => "Long".into(),
        Ty::F64 => "Double".into(),
        Ty::String => "String".into(),
        Ty::Unit => "Unit".into(),
        Ty::Option(t) => format!("{}?", kt(t)),
        Ty::List(t) => format!("List<{}>", kt(t)),
        Ty::Named(n) => n.clone(),
    }
}

fn encode_or_nothing(ty: &Ty, value: &str) -> String {
    if *ty == Ty::Unit {
        "Unit".into()
    } else {
        encode(ty, value, 1)
    }
}

/// Statement writing `value` (of `ty`) to `w`.
fn encode(ty: &Ty, value: &str, depth: usize) -> String {
    let v = format!("x{depth}");
    match ty {
        Ty::Bool => format!("w.bool({value})"),
        Ty::I32 => format!("w.int({value})"),
        Ty::I64 => format!("w.long({value})"),
        Ty::F64 => format!("w.double({value})"),
        Ty::String => format!("w.string({value})"),
        Ty::Unit => String::new(),
        Ty::Option(t) => format!(
            "w.optional({value}) {{ {v} -> {} }}",
            encode(t, &v, depth + 1)
        ),
        Ty::List(t) => format!("w.list({value}) {{ {v} -> {} }}", encode(t, &v, depth + 1)),
        Ty::Named(_) => format!("{value}.encode(w)"),
    }
}

/// Expression reading a `ty` from `r`.
fn decode(ty: &Ty) -> String {
    match ty {
        Ty::Bool => "r.bool()".into(),
        Ty::I32 => "r.int()".into(),
        Ty::I64 => "r.long()".into(),
        Ty::F64 => "r.double()".into(),
        Ty::String => "r.string()".into(),
        Ty::Unit => "Unit".into(),
        Ty::Option(t) => format!("r.optional {{ {} }}", decode(t)),
        Ty::List(t) => format!("r.list {{ {} }}", decode(t)),
        Ty::Named(n) => format!("{n}.decode(r)"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::schema::parse;
    use super::file;

    #[test]
    fn generates_types_interface_and_dispatcher() {
        let schema = parse(
            r#"
            #[derive(NativeType)] pub struct Info { pub user_name: String, pub level: Option<i32>, pub tags: Vec<String> }
            #[derive(NativeType)] pub enum Kind { Phone, SmartWatch }
            #[host_api] pub trait DemoApi {
                fn info(&self) -> Result<Info, E>;
                #[async_call] fn save_name(&self, user_name: String) -> Result<(), E>;
            }"#,
        )
        .unwrap();
        let kt = file("my-plugin", &schema);
        for expected in [
            "package dev.rux.generated.my_plugin",
            "data class Info(\n    val userName: String,\n    val level: Int?,\n    val tags: List<String>,\n)",
            "w.optional(level) { x0 -> w.int(x0) }",
            "level = r.optional { r.int() },",
            "enum class Kind {\n    PHONE,\n    SMART_WATCH;",
            "    fun info(): Info\n",
            "    fun saveName(userName: String, callback: (Result<Unit>) -> Unit)\n",
            "const val NAME = \"my-plugin.DemoApi\"",
            "0 -> { val v = impl.info(); reply.success { w -> v.encode(w) } }",
            "1 -> { val userName = r.string(); impl.saveName(userName, { result -> reply.complete(result) { w, v -> Unit } }) }",
        ] {
            assert!(
                kt.contains(expected),
                "missing:\n{expected}\n--- in ---\n{kt}"
            );
        }
    }
}
