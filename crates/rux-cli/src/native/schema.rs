//! Reads native API declarations (`#[rux::host_api]` traits, `#[derive(NativeType)]` types)
//! from a crate's sources — the same declarations `rux-macros` expands, so the API name and
//! method indices (declaration order) must agree with it.

use std::path::Path;

use anyhow::{Context, Result, bail};
use syn::{Attribute, Fields, FnArg, GenericArgument, Item, Pat, PathArguments, ReturnType, Type};

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Bool,
    I32,
    I64,
    F64,
    String,
    Unit,
    Option(Box<Ty>),
    List(Box<Ty>),
    Named(String),
}

#[derive(Debug, PartialEq)]
pub enum TypeDef {
    Struct {
        name: String,
        fields: Vec<(String, Ty)>,
    },
    Enum {
        name: String,
        variants: Vec<String>,
    },
}

impl TypeDef {
    pub fn name(&self) -> &str {
        match self {
            TypeDef::Struct { name, .. } | TypeDef::Enum { name, .. } => name,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Method {
    pub name: String,
    pub args: Vec<(String, Ty)>,
    pub ret: Ty,
    pub is_async: bool,
}

#[derive(Debug, PartialEq)]
pub struct Api {
    pub name: String,
    pub methods: Vec<Method>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Schema {
    pub types: Vec<TypeDef>,
    pub apis: Vec<Api>,
}

impl Schema {
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.apis.is_empty()
    }
}

/// Reads every `.rs` file under `src`.
pub fn read(src: &Path) -> Result<Schema> {
    let mut schema = Schema::default();
    read_dir(src, &mut schema)?;
    validate(&schema)?;
    Ok(schema)
}

fn read_dir(dir: &Path, schema: &mut Schema) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_dir(&path, schema)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = std::fs::read_to_string(&path)?;
            let file =
                syn::parse_file(&source).with_context(|| format!("parsing {}", path.display()))?;
            read_items(&file.items, schema).with_context(|| format!("in {}", path.display()))?;
        }
    }
    Ok(())
}

/// Parses declarations from source text (one file).
#[cfg(test)]
pub fn parse(source: &str) -> Result<Schema> {
    let mut schema = Schema::default();
    read_items(&syn::parse_file(source)?.items, &mut schema)?;
    validate(&schema)?;
    Ok(schema)
}

fn read_items(items: &[Item], schema: &mut Schema) -> Result<()> {
    for item in items {
        match item {
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    read_items(items, schema)?;
                }
            }
            Item::Trait(t) if has_attr(&t.attrs, "host_api") => {
                let mut methods = Vec::new();
                for item in &t.items {
                    let syn::TraitItem::Fn(f) = item else {
                        bail!("#[host_api] {} may only contain methods", t.ident);
                    };
                    methods.push(
                        read_method(f)
                            .with_context(|| format!("in {}::{}", t.ident, f.sig.ident))?,
                    );
                }
                schema.apis.push(Api {
                    name: t.ident.to_string(),
                    methods,
                });
            }
            Item::Struct(s) if derives_native_type(&s.attrs) => {
                let Fields::Named(fields) = &s.fields else {
                    bail!("NativeType {} needs named fields", s.ident);
                };
                let fields = fields
                    .named
                    .iter()
                    .map(|f| {
                        let name = f.ident.as_ref().unwrap().to_string();
                        let ty =
                            read_ty(&f.ty).with_context(|| format!("field {}::{name}", s.ident))?;
                        if ty == Ty::Unit {
                            bail!("field {}::{name} can't be `()`", s.ident);
                        }
                        Ok((name, ty))
                    })
                    .collect::<Result<_>>()?;
                schema.types.push(TypeDef::Struct {
                    name: s.ident.to_string(),
                    fields,
                });
            }
            Item::Enum(e) if derives_native_type(&e.attrs) => {
                schema.types.push(TypeDef::Enum {
                    name: e.ident.to_string(),
                    variants: e.variants.iter().map(|v| v.ident.to_string()).collect(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn read_method(f: &syn::TraitItemFn) -> Result<Method> {
    let mut args = Vec::new();
    for input in f.sig.inputs.iter().skip(1) {
        let FnArg::Typed(arg) = input else { continue };
        let Pat::Ident(pat) = &*arg.pat else {
            bail!("use a plain argument name");
        };
        args.push((pat.ident.to_string(), read_ty(&arg.ty)?));
    }
    let ReturnType::Type(_, ret) = &f.sig.output else {
        bail!("must return Result<T, NativeError>");
    };
    Ok(Method {
        name: f.sig.ident.to_string(),
        args,
        ret: result_ok(ret)?,
        is_async: has_attr(&f.attrs, "async_call"),
    })
}

fn result_ok(ty: &Type) -> Result<Ty> {
    if let Type::Path(p) = ty
        && let Some(last) = p.path.segments.last()
        && last.ident == "Result"
        && let PathArguments::AngleBracketed(args) = &last.arguments
        && let Some(GenericArgument::Type(ok)) = args.args.first()
    {
        return read_ty(ok);
    }
    bail!("must return Result<T, NativeError>")
}

fn read_ty(ty: &Type) -> Result<Ty> {
    match ty {
        Type::Tuple(t) if t.elems.is_empty() => Ok(Ty::Unit),
        Type::Path(p) if p.qself.is_none() => {
            let last = p.path.segments.last().context("empty type path")?;
            let inner = || -> Result<Ty> {
                if let PathArguments::AngleBracketed(args) = &last.arguments
                    && let Some(GenericArgument::Type(t)) = args.args.first()
                {
                    return read_ty(t);
                }
                bail!("{} needs a type argument", last.ident)
            };
            Ok(match last.ident.to_string().as_str() {
                "bool" => Ty::Bool,
                "i32" => Ty::I32,
                "i64" => Ty::I64,
                "f64" => Ty::F64,
                "String" => Ty::String,
                "Option" => match inner()? {
                    Ty::Option(_) => bail!("nested Option is not supported"),
                    t => Ty::Option(Box::new(t)),
                },
                "Vec" => Ty::List(Box::new(inner()?)),
                name if last.arguments.is_empty() => Ty::Named(name.to_owned()),
                name => bail!("unsupported generic type {name}"),
            })
        }
        _ => bail!(
            "unsupported type (use bool, i32, i64, f64, String, Option, Vec, or a NativeType)"
        ),
    }
}

fn validate(schema: &Schema) -> Result<()> {
    fn check(ty: &Ty, schema: &Schema, at: &str) -> Result<()> {
        match ty {
            Ty::Option(t) | Ty::List(t) => check(t, schema, at),
            Ty::Named(n) if !schema.types.iter().any(|t| t.name() == n) => {
                bail!("{at}: `{n}` is not a #[derive(NativeType)] type of this crate")
            }
            _ => Ok(()),
        }
    }
    for t in &schema.types {
        if let TypeDef::Struct { name, fields } = t {
            for (f, ty) in fields {
                check(ty, schema, &format!("{name}::{f}"))?;
            }
        }
    }
    for api in &schema.apis {
        for m in &api.methods {
            let at = format!("{}::{}", api.name, m.name);
            check(&m.ret, schema, &at)?;
            for (_, ty) in &m.args {
                check(ty, schema, &at)?;
            }
        }
    }
    Ok(())
}

/// `#[name]` or `#[path::name]`.
fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == name))
}

fn derives_native_type(attrs: &[Attribute]) -> bool {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("derive"))
        .any(|a| {
            let mut found = false;
            let _ = a.parse_nested_meta(|meta| {
                found |= meta
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == "NativeType");
                Ok(())
            });
            found
        })
}

#[cfg(test)]
mod tests {
    use super::{Ty, TypeDef, parse};

    #[test]
    fn reads_apis_and_types() {
        let schema = parse(
            r#"
            #[derive(Debug, rux::NativeType)]
            pub struct Info { pub name: String, pub level: Option<i32>, pub tags: Vec<Kind> }
            #[derive(NativeType)]
            pub enum Kind { A, B }
            struct NotNative { x: i32 }

            #[rux::host_api]
            pub trait DemoApi {
                fn info(&self) -> Result<Info, NativeError>;
                #[rux::async_call]
                fn save(&self, name: String, big: i64) -> Result<(), NativeError>;
            }
            "#,
        )
        .unwrap();
        assert_eq!(schema.types.len(), 2);
        assert_eq!(
            schema.types[0],
            TypeDef::Struct {
                name: "Info".into(),
                fields: vec![
                    ("name".into(), Ty::String),
                    ("level".into(), Ty::Option(Box::new(Ty::I32))),
                    ("tags".into(), Ty::List(Box::new(Ty::Named("Kind".into())))),
                ],
            }
        );
        let api = &schema.apis[0];
        assert_eq!(api.name, "DemoApi");
        assert!(!api.methods[0].is_async);
        assert_eq!(api.methods[0].ret, Ty::Named("Info".into()));
        assert!(api.methods[1].is_async);
        assert_eq!(
            api.methods[1].args,
            [("name".into(), Ty::String), ("big".into(), Ty::I64)]
        );
        assert_eq!(api.methods[1].ret, Ty::Unit);
    }

    #[test]
    fn rejects_unsupported_declarations() {
        let unknown = "#[host_api] trait A { fn f(&self) -> Result<Missing, E>; }";
        assert!(parse(unknown).is_err());
        let bad_ret = "#[host_api] trait A { fn f(&self) -> i32; }";
        assert!(parse(bad_ret).is_err());
        // `u8` isn't a supported primitive, nor a NativeType of the crate.
        let bad_ty = "#[host_api] trait A { fn f(&self, x: u8) -> Result<(), E>; }";
        assert!(parse(bad_ty).is_err());
        assert!(parse(&bad_ty.replace("u8", "&str")).is_err());
        let nested = "#[host_api] trait A { fn f(&self) -> Result<Option<Option<i32>>, E>; }";
        assert!(parse(nested).is_err());
    }
}
