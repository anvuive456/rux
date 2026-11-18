//! Procedural macros for rux (re-exported by the `rux` crate).
//!
//! `rux build` reads the same declarations (with `syn`) to generate the Kotlin / Swift side,
//! so the API name (`"<crate>.<Trait>"`) and method indices (declaration order) below must stay
//! in sync with `rux-cli/src/native`.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    Data, DeriveInput, Error, Fields, FnArg, GenericArgument, ItemTrait, Pat, PathArguments,
    ReturnType, TraitItem, Type, parse_macro_input,
};

/// Declares an API implemented natively (Kotlin / Swift) and called from Rust.
///
/// ```ignore
/// #[rux::host_api]
/// pub trait DeviceInfoApi {
///     fn info(&self) -> Result<DeviceInfo, NativeError>;
///     #[rux::async_call]
///     fn battery(&self) -> Result<Battery, NativeError>;
/// }
/// ```
///
/// Becomes a client struct: `DeviceInfoApi::new().info()` (sync) and
/// `DeviceInfoApi::new().battery(|result| …)` (async: the callback runs on the UI thread).
/// Every method takes `&self` plus arguments, and returns `Result<T, NativeError>`.
#[proc_macro_attribute]
pub fn host_api(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(
            proc_macro2::Span::call_site(),
            "#[host_api] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let item = parse_macro_input!(item as ItemTrait);
    host_api_impl(item)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Marks a `#[host_api]` method as asynchronous: the native side answers later, and the Rust
/// client takes a callback. Only meaningful inside `#[host_api]`.
#[proc_macro_attribute]
pub fn async_call(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Makes a struct (named fields) or enum (unit variants) passable to native code; `rux build`
/// generates the matching Kotlin / Swift type.
#[proc_macro_derive(NativeType)]
pub fn native_type(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    native_type_impl(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

fn host_api_impl(item: ItemTrait) -> syn::Result<TokenStream2> {
    let name = &item.ident;
    let vis = &item.vis;
    let docs: Vec<_> = item
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .collect();
    let api = name.to_string();

    let mut methods = Vec::new();
    for (index, trait_item) in item.items.iter().enumerate() {
        // Method index on the wire (declaration order), as in the generated Kotlin / Swift.
        let index = index as u32;
        let TraitItem::Fn(f) = trait_item else {
            return Err(Error::new(
                trait_item.span(),
                "#[host_api] traits may only contain methods",
            ));
        };
        let sig = &f.sig;
        let method = &sig.ident;
        let method_docs: Vec<_> = f
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("doc"))
            .collect();
        let is_async = f.attrs.iter().any(|a| {
            a.path()
                .segments
                .last()
                .is_some_and(|s| s.ident == "async_call")
        });
        if sig.asyncness.is_some() || !sig.generics.params.is_empty() {
            return Err(Error::new(
                sig.span(),
                "use #[rux::async_call] (not `async fn`), and no generics",
            ));
        }

        let mut inputs = sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(r)) if r.reference.is_some() && r.mutability.is_none() => {}
            _ => return Err(Error::new(sig.span(), "methods must take `&self` first")),
        }
        let mut params = Vec::new();
        let mut encodes = Vec::new();
        for input in inputs {
            let FnArg::Typed(arg) = input else {
                unreachable!()
            };
            let Pat::Ident(pat) = &*arg.pat else {
                return Err(Error::new(arg.pat.span(), "use a plain argument name"));
            };
            let (ident, ty) = (&pat.ident, &arg.ty);
            params.push(quote!(#ident: #ty));
            encodes.push(quote!(::rux::native::Codec::encode(&#ident, &mut __w);));
        }
        let ok = result_ok_type(&sig.output)?;
        // The return type as written (`Result<T, NativeError>`, maybe through an alias or
        // import): keeps the user's imports used, and mismatched error types fail to compile.
        let ReturnType::Type(_, ret) = &sig.output else {
            unreachable!("checked by result_ok_type")
        };

        let body_args = quote! {
            #[allow(unused_mut)]
            let mut __w = ::rux::native::Writer::new();
            #(#encodes)*
        };
        methods.push(if is_async {
            quote! {
                #(#method_docs)*
                pub fn #method(
                    &self,
                    #(#params,)*
                    callback: impl FnOnce(#ret) + 'static,
                ) {
                    #body_args
                    ::rux::native::call_async(Self::API, #index, __w.finish(), move |r| {
                        callback(::rux::native::decode_result::<#ok>(r))
                    });
                }
            }
        } else {
            quote! {
                #(#method_docs)*
                pub fn #method(&self, #(#params),*)
                    -> #ret
                {
                    #body_args
                    ::rux::native::decode_result::<#ok>(
                        ::rux::native::call_sync(Self::API, #index, __w.finish()),
                    )
                }
            }
        });
    }

    Ok(quote! {
        #(#docs)*
        ///
        /// Native API client (generated by `#[rux::host_api]`): implemented in Kotlin / Swift.
        #[derive(Clone, Copy, Debug, Default)]
        #vis struct #name;

        impl #name {
            /// Native API name, `"<crate>.<Trait>"` (matches the generated Kotlin / Swift).
            pub const API: &'static str = concat!(env!("CARGO_PKG_NAME"), ".", #api);

            pub fn new() -> Self {
                Self
            }

            #(#methods)*
        }
    })
}

/// `T` from `-> Result<T, NativeError>`.
fn result_ok_type(output: &ReturnType) -> syn::Result<Type> {
    let err = || {
        Error::new(
            output.span(),
            "native methods must return `Result<T, NativeError>`",
        )
    };
    let ReturnType::Type(_, ty) = output else {
        return Err(err());
    };
    let Type::Path(path) = &**ty else {
        return Err(err());
    };
    let last = path.path.segments.last().ok_or_else(err)?;
    if last.ident != "Result" {
        return Err(err());
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(err());
    };
    match args.args.first() {
        Some(GenericArgument::Type(t)) if args.args.len() == 2 => Ok(t.clone()),
        _ => Err(err()),
    }
}

fn native_type_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "NativeType can't be generic",
        ));
    }
    let (encode, decode) = match &input.data {
        Data::Struct(s) => {
            let Fields::Named(fields) = &s.fields else {
                return Err(Error::new(
                    s.fields.span(),
                    "NativeType structs need named fields",
                ));
            };
            let names: Vec<_> = fields.named.iter().map(|f| &f.ident).collect();
            (
                quote! { #(::rux::native::Codec::encode(&self.#names, w);)* },
                quote! { Ok(Self { #(#names: ::rux::native::Codec::decode(r)?,)* }) },
            )
        }
        Data::Enum(e) => {
            let mut variants = Vec::new();
            for v in &e.variants {
                if !matches!(v.fields, Fields::Unit) || v.discriminant.is_some() {
                    return Err(Error::new(
                        v.span(),
                        "NativeType enums may only have plain unit variants",
                    ));
                }
                variants.push(&v.ident);
            }
            let indices: Vec<i32> = (0..variants.len() as i32).collect();
            let error = format_ident!("__rux_bad_variant");
            (
                quote! {
                    let index: i32 = match self { #(Self::#variants => #indices,)* };
                    ::rux::native::Codec::encode(&index, w);
                },
                quote! {
                    let #error = <i32 as ::rux::native::Codec>::decode(r)?;
                    match #error {
                        #(#indices => Ok(Self::#variants),)*
                        _ => Err(::rux::native::DecodeError::new(
                            format!("invalid {} variant {}", stringify!(#name), #error),
                        )),
                    }
                },
            )
        }
        Data::Union(u) => {
            return Err(Error::new(
                u.union_token.span(),
                "NativeType can't be a union",
            ));
        }
    };
    Ok(quote! {
        impl ::rux::native::Codec for #name {
            fn encode(&self, w: &mut ::rux::native::Writer) {
                #encode
            }
            fn decode(
                r: &mut ::rux::native::Reader,
            ) -> ::std::result::Result<Self, ::rux::native::DecodeError> {
                #decode
            }
        }
    })
}
