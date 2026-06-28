//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Pierre Avital, <pierre.avital@me.com>
//

use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    FnArg, GenericParam, ImplItem, ImplItemFn, ItemImpl, ItemStruct, ItemTrait, LitStr, ReturnType,
    Token, TraitItem, TraitItemFn, Type,
};

enum Receiver {
    Opaque(Type),
    Stable(Type),
}

struct InterfaceArgs {
    receiver: Receiver,
    prefix: String,
    link_args: TokenStream,
    has_link_args: bool,
    vtable: Option<Type>,
    resolver: bool,
}

impl Parse for InterfaceArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut receiver = None;
        let mut prefix = None;
        let mut link_args = quote!();
        let mut has_link_args = false;
        let mut vtable = None;
        let mut resolver = false;
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            if ident == "resolver" && !input.peek(Token![=]) {
                resolver = true;
                _ = input.parse::<Token![,]>();
                continue;
            }
            input.parse::<Token![=]>()?;
            match ident.to_string().as_str() {
                "opaque" => {
                    receiver = Some(Receiver::Opaque(input.parse()?));
                }
                "receiver" => {
                    receiver = Some(Receiver::Stable(input.parse()?));
                }
                "prefix" => {
                    prefix = Some(input.parse::<LitStr>()?.value());
                }
                "vtable" => {
                    vtable = Some(input.parse()?);
                }
                _ => {
                    let lit: LitStr = input.parse()?;
                    link_args.extend(quote!(#ident = #lit,));
                    has_link_args = true;
                }
            }
            _ = input.parse::<Token![,]>();
        }
        Ok(Self {
            receiver: receiver
                .ok_or_else(|| input.error("expected `opaque = Type` or `receiver = Type`"))?,
            prefix: prefix.ok_or_else(|| input.error("expected `prefix = \"symbol_prefix\"`"))?,
            link_args,
            has_link_args,
            vtable,
            resolver,
        })
    }
}

struct OpaqueArgs {
    version: u32,
    module: TokenStream,
}

impl Parse for OpaqueArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut this = Self {
            version: 0,
            module: quote!(),
        };
        while !input.is_empty() {
            let ident: Ident = input.parse()?;
            match ident.to_string().as_str() {
                "version" => {
                    input.parse::<Token![=]>()?;
                    this.version = input.parse::<syn::LitInt>()?.base10_parse()?;
                }
                "module" => {
                    input.parse::<Token![=]>()?;
                    while !input.is_empty() {
                        if input.peek(Token![,]) {
                            break;
                        }
                        let token: proc_macro2::TokenTree = input.parse()?;
                        this.module.extend(Some(token));
                    }
                }
                _ => return Err(syn::Error::new(ident.span(), "unknown opaque attribute")),
            }
            _ = input.parse::<Token![,]>();
        }
        Ok(this)
    }
}

struct Arg {
    ident: Ident,
    ty: Type,
}

struct Method {
    ident: Ident,
    symbol: Ident,
    generics: syn::Generics,
    unsafety: Option<Token![unsafe]>,
    abi: syn::Abi,
    receiver_lt: Option<syn::Lifetime>,
    receiver_mut: bool,
    args: Vec<Arg>,
    output: ReturnType,
}

impl Method {
    fn from_trait(value: &TraitItemFn, prefix: &str) -> Self {
        Self::from_parts(
            &value.sig.ident,
            &value.sig.generics,
            value.sig.unsafety,
            value.sig.abi.clone(),
            value.sig.inputs.iter(),
            &value.sig.output,
            prefix,
        )
    }

    fn from_impl(value: &ImplItemFn, prefix: &str) -> Self {
        Self::from_parts(
            &value.sig.ident,
            &value.sig.generics,
            value.sig.unsafety,
            value.sig.abi.clone(),
            value.sig.inputs.iter(),
            &value.sig.output,
            prefix,
        )
    }

    fn from_parts<'a>(
        ident: &Ident,
        generics: &syn::Generics,
        unsafety: Option<Token![unsafe]>,
        abi: Option<syn::Abi>,
        mut inputs: impl Iterator<Item = &'a FnArg>,
        output: &ReturnType,
        prefix: &str,
    ) -> Self {
        if generics
            .params
            .iter()
            .any(|param| !matches!(param, GenericParam::Lifetime(_)))
        {
            panic!("stabby interfaces only support lifetime method generics");
        }
        if generics.where_clause.is_some() {
            panic!("stabby interfaces don't support method where-clauses yet");
        }
        let abi = abi.unwrap_or_else(|| {
            panic!("stabby interface methods must use an explicit stable calling convention")
        });
        match abi.name.as_ref().map(|name| name.value()) {
            Some(name) if name == "C" => {}
            _ => panic!("stabby interfaces currently support only `extern \"C\"` methods"),
        }
        let Some(FnArg::Receiver(receiver)) = inputs.next() else {
            panic!("stabby interface methods must take `&self` or `&mut self`")
        };
        let Some((_, receiver_lt)) = &receiver.reference else {
            panic!("stabby interface methods must take `&self` or `&mut self`")
        };
        let args = inputs
            .enumerate()
            .map(|(i, arg)| {
                let FnArg::Typed(arg) = arg else {
                    panic!("only the first argument may be a receiver")
                };
                Arg {
                    ident: format_ident!("_stabby_arg_{i}"),
                    ty: (*arg.ty).clone(),
                }
            })
            .collect();
        Self {
            ident: ident.clone(),
            symbol: symbol_ident(prefix, ident),
            generics: generics.clone(),
            unsafety,
            abi,
            receiver_lt: receiver_lt.clone(),
            receiver_mut: receiver.mutability.is_some(),
            args,
            output: output.clone(),
        }
    }

    fn receiver_lt(&self) -> TokenStream {
        self.receiver_lt
            .as_ref()
            .map(ToTokens::to_token_stream)
            .unwrap_or_else(|| quote!('_))
    }

    fn arg_decls(&self) -> Vec<TokenStream> {
        self.args
            .iter()
            .map(|Arg { ident, ty }| quote!(#ident: #ty))
            .collect()
    }

    fn erased_arg_decls(&self) -> Vec<TokenStream> {
        self.args
            .iter()
            .map(|Arg { ident, ty }| {
                let ty = erase_lifetimes_type(ty);
                quote!(#ident: #ty)
            })
            .collect()
    }

    fn arg_names(&self) -> Vec<&Ident> {
        self.args.iter().map(|arg| &arg.ident).collect()
    }

    fn erased_output(&self) -> ReturnType {
        erase_lifetimes_return_type(&self.output)
    }

    fn foreign_receiver(&self, receiver: &Receiver, st: &TokenStream) -> TokenStream {
        let lt = self.receiver_lt();
        match (receiver, self.receiver_mut) {
            (Receiver::Opaque(opaque), false) => quote!(#st::opaque::Ref<#opaque>),
            (Receiver::Opaque(opaque), true) => quote!(#st::opaque::RefMut<#opaque>),
            (Receiver::Stable(receiver), false) => quote!(&#lt #receiver),
            (Receiver::Stable(receiver), true) => quote!(&#lt mut #receiver),
        }
    }

    fn trait_receiver(&self) -> TokenStream {
        match (&self.receiver_lt, self.receiver_mut) {
            (Some(lt), false) => quote!(&#lt self),
            (Some(lt), true) => quote!(&#lt mut self),
            (None, false) => quote!(&self),
            (None, true) => quote!(&mut self),
        }
    }

    fn import_foreign_item(&self, receiver: &Receiver, st: &TokenStream) -> TokenStream {
        let symbol = &self.symbol;
        let output = self.erased_output();
        let arg_decls = self.erased_arg_decls();
        let receiver = self.foreign_receiver(receiver, st);
        quote!(pub fn #symbol(this: #receiver, #(#arg_decls),*) #output;)
    }

    fn vtable_fn_type(&self, receiver: &Receiver, st: &TokenStream) -> Type {
        let output = self.erased_output();
        let receiver = self.foreign_receiver(receiver, st);
        let arg_tys = self
            .args
            .iter()
            .map(|Arg { ty, .. }| erase_lifetimes_type(ty));
        let args = core::iter::once(quote!(#receiver))
            .chain(arg_tys.map(|ty| quote!(#ty)))
            .collect::<Vec<_>>();
        let unsafety = self.unsafety;
        let abi = &self.abi;
        syn::parse2(quote!(#unsafety #abi fn(#(#args),*) #output))
            .unwrap_or_else(|e| panic!("failed to parse generated vtable fn type: {e}"))
    }

    fn imported_method(
        &self,
        receiver: &Receiver,
        _st: &TokenStream,
        ref_mut: bool,
    ) -> TokenStream {
        let ident = &self.ident;
        let symbol = &self.symbol;
        let generics = &self.generics;
        let unsafety = self.unsafety;
        let abi = &self.abi;
        let output = &self.output;
        let arg_decls = self.arg_decls();
        let arg_names = self.arg_names();
        let arg_casts = self.args.iter().map(|Arg { ident, ty }| {
            let erased = erase_lifetimes_type(ty);
            quote!(let #ident: #erased = unsafe { ::core::mem::transmute(#ident) };)
        });
        let receiver_arg = match (receiver, ref_mut, self.receiver_mut) {
            (Receiver::Opaque(_), false, false) => quote!(*self),
            (Receiver::Opaque(_), true, false) => quote!(self.as_ref()),
            (Receiver::Opaque(_), true, true) => quote!(self.reborrow()),
            (Receiver::Stable(_), _, _) => quote!(self),
            (Receiver::Opaque(_), false, true) => unreachable!(),
        };
        let call = quote!(#symbol(#receiver_arg, #(#arg_names),*));
        let call = if unsafety.is_some() {
            quote!(unsafe { #call })
        } else {
            call
        };
        let body = match output {
            ReturnType::Default => quote! {
                #(#arg_casts)*
                #call
            },
            ReturnType::Type(_, ty) => {
                let erased = erase_lifetimes_type(ty);
                quote! {
                    #(#arg_casts)*
                    let _stabby_result: #erased = #call;
                    unsafe { ::core::mem::transmute(_stabby_result) }
                }
            }
        };
        let receiver = self.trait_receiver();
        quote! {
            #unsafety #abi fn #ident #generics(#receiver, #(#arg_decls),*) #output {
                #body
            }
        }
    }

    fn bound_method(&self, ref_mut: bool) -> TokenStream {
        let ident = &self.ident;
        let generics = &self.generics;
        let unsafety = self.unsafety;
        let abi = &self.abi;
        let output = &self.output;
        let arg_decls = self.arg_decls();
        let arg_names = self.arg_names();
        let arg_casts = self.args.iter().map(|Arg { ident, ty }| {
            let erased = erase_lifetimes_type(ty);
            quote!(let #ident: #erased = unsafe { ::core::mem::transmute(#ident) };)
        });
        let receiver_arg = match (ref_mut, self.receiver_mut) {
            (_, false) => quote!(self.as_opaque()),
            (true, true) => quote!(self.as_opaque_mut()),
            (false, true) => unreachable!(),
        };
        let call_args = core::iter::once(receiver_arg)
            .chain(arg_names.iter().map(|ident| quote!(#ident)))
            .collect::<Vec<_>>();
        let call = quote!((self.vtable().#ident)(#(#call_args),*));
        let call = if unsafety.is_some() {
            quote!(unsafe { #call })
        } else {
            call
        };
        let body = match output {
            ReturnType::Default => quote! {
                #(#arg_casts)*
                #call
            },
            ReturnType::Type(_, ty) => {
                let erased = erase_lifetimes_type(ty);
                quote! {
                    #(#arg_casts)*
                    let _stabby_result: #erased = #call;
                    unsafe { ::core::mem::transmute(_stabby_result) }
                }
            }
        };
        let receiver = self.trait_receiver();
        quote! {
            #unsafety #abi fn #ident #generics(#receiver, #(#arg_decls),*) #output {
                #body
            }
        }
    }

    fn export_item(
        &self,
        receiver: &Receiver,
        st: &TokenStream,
        trait_path: &syn::Path,
        self_ty: &Type,
    ) -> TokenStream {
        let symbol = &self.symbol;
        let unsafety = self.unsafety;
        let abi = &self.abi;
        let output = self.erased_output();
        let arg_decls = self.erased_arg_decls();
        let arg_names = self.arg_names();
        let receiver_ty = self.foreign_receiver(receiver, st);
        let this_decl = quote!(this: #receiver_ty);
        let this_binding = self.receiver_mut.then(|| quote!(let mut this = this;));
        let receiver = match (receiver, self.receiver_mut) {
            (Receiver::Opaque(_), false) => quote!(unsafe { this.cast::<#self_ty>() }),
            (Receiver::Opaque(_), true) => quote!(unsafe { this.cast_mut::<#self_ty>() }),
            (Receiver::Stable(_), _) => quote!(this),
        };
        let ident = &self.ident;
        let call = quote!(<#self_ty as #trait_path>::#ident(#receiver, #(#arg_names),*));
        let call = if unsafety.is_some() {
            quote!(unsafe { #call })
        } else {
            call
        };
        let body = match &self.output {
            ReturnType::Default => call,
            ReturnType::Type(_, ty) => {
                let erased = erase_lifetimes_type(ty);
                quote! {
                    let _stabby_result = #call;
                    unsafe { ::core::mem::transmute::<_, #erased>(_stabby_result) }
                }
            }
        };
        let item_tokens = quote! {
            pub #unsafety #abi fn #symbol(#this_decl, #(#arg_decls),*) #output {
                #this_binding
                #body
            }
        };
        let item: syn::ItemFn = syn::parse2(item_tokens.clone())
            .unwrap_or_else(|e| panic!("failed to parse generated export `{item_tokens}`: {e}"));
        crate::functions::export(proc_macro::TokenStream::new(), item)
    }
}

fn interface_vtable_items(
    args: &InterfaceArgs,
    methods: &[Method],
    st: &TokenStream,
) -> Option<TokenStream> {
    let vtable = args.vtable.as_ref()?;
    let Receiver::Opaque(opaque) = &args.receiver else {
        panic!("runtime interface vtables currently require `opaque = Type`");
    };
    let symbols = methods.iter().map(|method| &method.symbol);
    let vtable_static = symbol_ident(&args.prefix, &format_ident!("interface_vtable"));
    let bind_fn = symbol_ident(&args.prefix, &format_ident!("interface_bind"));
    let bind_erased_fn = symbol_ident(&args.prefix, &format_ident!("interface_bind_erased"));
    let query_fn = symbol_ident(&args.prefix, &format_ident!("interface_query"));
    let bind_ref_fn = methods
        .iter()
        .all(|method| !method.receiver_mut)
        .then(|| symbol_ident(&args.prefix, &format_ident!("interface_bind_ref")));
    let bind_ref = bind_ref_fn.map(|bind_ref_fn| {
        let bind_ref_erased_fn =
            symbol_ident(&args.prefix, &format_ident!("interface_bind_ref_erased"));
        let query_ref_fn = symbol_ident(&args.prefix, &format_ident!("interface_query_ref"));
        quote! {
            #[allow(missing_docs)]
            pub fn #bind_ref_fn(this: #st::opaque::Ref<#opaque>) -> #st::opaque::InterfaceRef<#opaque, #vtable> {
                #st::opaque::InterfaceRef::new(this, &#vtable_static)
            }

            #[allow(missing_docs)]
            pub fn #bind_ref_erased_fn(this: #st::opaque::Ref<#opaque>) -> #st::opaque::ErasedInterfaceRef<#opaque> {
                #bind_ref_fn(this).erase()
            }

            #[allow(missing_docs)]
            pub fn #query_ref_fn(
                this: #st::opaque::Ref<#opaque>,
                interface_id: u64,
                expected: &'static #st::report::TypeReport,
            ) -> #st::option::Option<#st::opaque::ErasedInterfaceRef<#opaque>> {
                if interface_id == <#vtable as #st::IStable>::ID
                    && <#vtable as #st::IStable>::REPORT.is_compatible(expected)
                {
                    #st::option::Option::Some(#bind_ref_erased_fn(this))
                } else {
                    #st::option::Option::None()
                }
            }
        }
    });
    Some(quote! {
        #[allow(non_upper_case_globals, missing_docs)]
        pub static #vtable_static: #vtable = #vtable::new(#(#symbols),*);

        #[allow(missing_docs)]
        pub fn #bind_fn(this: #st::opaque::RefMut<#opaque>) -> #st::opaque::InterfaceRefMut<#opaque, #vtable> {
            #st::opaque::InterfaceRefMut::new(this, &#vtable_static)
        }

        #[allow(missing_docs)]
        pub fn #bind_erased_fn(this: #st::opaque::RefMut<#opaque>) -> #st::opaque::ErasedInterfaceRefMut<#opaque> {
            #bind_fn(this).erase()
        }

        #[allow(missing_docs)]
        pub fn #query_fn(
            this: &mut #st::opaque::RefMut<#opaque>,
            interface_id: u64,
            expected: &'static #st::report::TypeReport,
        ) -> #st::option::Option<#st::opaque::ErasedInterfaceRefMut<#opaque>> {
            if interface_id == <#vtable as #st::IStable>::ID
                && <#vtable as #st::IStable>::REPORT.is_compatible(expected)
            {
                #st::option::Option::Some(#bind_erased_fn(this.reborrow()))
            } else {
                #st::option::Option::None()
            }
        }

        #bind_ref
    })
}

fn static_lifetime() -> syn::Lifetime {
    syn::Lifetime::new("'static", Span::call_site())
}

fn erase_lifetimes_return_type(output: &ReturnType) -> ReturnType {
    match output {
        ReturnType::Default => ReturnType::Default,
        ReturnType::Type(arrow, ty) => ReturnType::Type(*arrow, Box::new(erase_lifetimes_type(ty))),
    }
}

fn erase_lifetimes_type(ty: &Type) -> Type {
    match ty {
        Type::Array(ty) => Type::Array(syn::TypeArray {
            bracket_token: ty.bracket_token,
            elem: Box::new(erase_lifetimes_type(&ty.elem)),
            semi_token: ty.semi_token,
            len: ty.len.clone(),
        }),
        Type::BareFn(ty) => Type::BareFn(syn::TypeBareFn {
            lifetimes: None,
            unsafety: ty.unsafety,
            abi: ty.abi.clone(),
            fn_token: ty.fn_token,
            paren_token: ty.paren_token,
            inputs: ty
                .inputs
                .iter()
                .map(|arg| syn::BareFnArg {
                    attrs: arg.attrs.clone(),
                    name: arg.name.clone(),
                    ty: erase_lifetimes_type(&arg.ty),
                })
                .collect(),
            variadic: ty.variadic.clone(),
            output: erase_lifetimes_return_type(&ty.output),
        }),
        Type::Group(ty) => Type::Group(syn::TypeGroup {
            group_token: ty.group_token,
            elem: Box::new(erase_lifetimes_type(&ty.elem)),
        }),
        Type::Paren(ty) => Type::Paren(syn::TypeParen {
            paren_token: ty.paren_token,
            elem: Box::new(erase_lifetimes_type(&ty.elem)),
        }),
        Type::Path(ty) => Type::Path(syn::TypePath {
            qself: ty.qself.as_ref().map(|qself| syn::QSelf {
                lt_token: qself.lt_token,
                ty: Box::new(erase_lifetimes_type(&qself.ty)),
                position: qself.position,
                as_token: qself.as_token,
                gt_token: qself.gt_token,
            }),
            path: erase_lifetimes_path(&ty.path),
        }),
        Type::Ptr(ty) => Type::Ptr(syn::TypePtr {
            star_token: ty.star_token,
            const_token: ty.const_token,
            mutability: ty.mutability,
            elem: Box::new(erase_lifetimes_type(&ty.elem)),
        }),
        Type::Reference(ty) => Type::Reference(syn::TypeReference {
            and_token: ty.and_token,
            lifetime: Some(static_lifetime()),
            mutability: ty.mutability,
            elem: Box::new(erase_lifetimes_type(&ty.elem)),
        }),
        Type::Slice(ty) => Type::Slice(syn::TypeSlice {
            bracket_token: ty.bracket_token,
            elem: Box::new(erase_lifetimes_type(&ty.elem)),
        }),
        Type::Tuple(ty) => Type::Tuple(syn::TypeTuple {
            paren_token: ty.paren_token,
            elems: ty.elems.iter().map(erase_lifetimes_type).collect(),
        }),
        _ => ty.clone(),
    }
}

fn erase_lifetimes_path(path: &syn::Path) -> syn::Path {
    syn::Path {
        leading_colon: path.leading_colon,
        segments: path
            .segments
            .iter()
            .map(|segment| syn::PathSegment {
                ident: segment.ident.clone(),
                arguments: erase_lifetimes_path_arguments(&segment.arguments),
            })
            .collect(),
    }
}

fn erase_lifetimes_path_arguments(arguments: &syn::PathArguments) -> syn::PathArguments {
    match arguments {
        syn::PathArguments::None => syn::PathArguments::None,
        syn::PathArguments::AngleBracketed(arguments) => {
            syn::PathArguments::AngleBracketed(erase_lifetimes_angle_arguments(arguments))
        }
        syn::PathArguments::Parenthesized(arguments) => {
            syn::PathArguments::Parenthesized(syn::ParenthesizedGenericArguments {
                paren_token: arguments.paren_token,
                inputs: arguments.inputs.iter().map(erase_lifetimes_type).collect(),
                output: erase_lifetimes_return_type(&arguments.output),
            })
        }
    }
}

fn erase_lifetimes_angle_arguments(
    arguments: &syn::AngleBracketedGenericArguments,
) -> syn::AngleBracketedGenericArguments {
    syn::AngleBracketedGenericArguments {
        colon2_token: arguments.colon2_token,
        lt_token: arguments.lt_token,
        args: arguments
            .args
            .iter()
            .map(erase_lifetimes_generic_argument)
            .collect(),
        gt_token: arguments.gt_token,
    }
}

fn erase_lifetimes_generic_argument(argument: &syn::GenericArgument) -> syn::GenericArgument {
    match argument {
        syn::GenericArgument::Lifetime(_) => syn::GenericArgument::Lifetime(static_lifetime()),
        syn::GenericArgument::Type(ty) => syn::GenericArgument::Type(erase_lifetimes_type(ty)),
        syn::GenericArgument::AssocType(assoc) => syn::GenericArgument::AssocType(syn::AssocType {
            ident: assoc.ident.clone(),
            generics: assoc.generics.as_ref().map(erase_lifetimes_angle_arguments),
            eq_token: assoc.eq_token,
            ty: erase_lifetimes_type(&assoc.ty),
        }),
        syn::GenericArgument::Constraint(constraint) => {
            syn::GenericArgument::Constraint(syn::Constraint {
                ident: constraint.ident.clone(),
                generics: constraint
                    .generics
                    .as_ref()
                    .map(erase_lifetimes_angle_arguments),
                colon_token: constraint.colon_token,
                bounds: constraint.bounds.clone(),
            })
        }
        other => other.clone(),
    }
}

fn symbol_ident(prefix: &str, ident: &Ident) -> Ident {
    let symbol = format!("{prefix}_{ident}");
    if !symbol
        .chars()
        .all(|c| c == '_' || c.is_ascii_alphanumeric())
        || symbol.chars().next().map_or(true, |c| c.is_ascii_digit())
    {
        panic!("`prefix` must produce valid Rust symbol identifiers");
    }
    Ident::new(&symbol, Span::call_site())
}

fn trait_methods(item: &ItemTrait, prefix: &str) -> Vec<Method> {
    if !item.generics.params.is_empty() {
        panic!("stabby interfaces don't support generic traits yet");
    }
    item.items
        .iter()
        .map(|item| match item {
            TraitItem::Fn(method) => Method::from_trait(method, prefix),
            _ => panic!("stabby interfaces only support methods"),
        })
        .collect()
}

fn impl_methods(item: &ItemImpl, prefix: &str) -> Vec<Method> {
    if !item.generics.params.is_empty() {
        panic!("stabby interfaces don't support generic impls yet");
    }
    item.items
        .iter()
        .filter_map(|item| match item {
            ImplItem::Fn(method) => Some(Method::from_impl(method, prefix)),
            _ => None,
        })
        .collect()
}

pub fn import_interface(
    attrs: proc_macro::TokenStream,
    item: ItemTrait,
) -> proc_macro2::TokenStream {
    let args: InterfaceArgs = syn::parse(attrs).unwrap();
    if !args.has_link_args {
        panic!("`stabby::import_interface` requires link arguments such as `name = \"library\"`");
    }
    let st = crate::tl_mod();
    let methods = trait_methods(&item, &args.prefix);
    let foreign_items = methods
        .iter()
        .map(|method| method.import_foreign_item(&args.receiver, &st));
    let import_block: syn::ItemForeignMod = syn::parse2(quote! {
        extern "C" {
            #(#foreign_items)*
        }
    })
    .unwrap();
    let imports = crate::functions::import(args.link_args.into(), import_block);
    let trait_ident = &item.ident;
    let ref_impl = match &args.receiver {
        Receiver::Opaque(opaque) if methods.iter().all(|method| !method.receiver_mut) => {
            let methods = methods
                .iter()
                .map(|method| method.imported_method(&args.receiver, &st, false));
            Some(quote! {
                impl #trait_ident for #st::opaque::Ref<#opaque> {
                    #(#methods)*
                }
            })
        }
        _ => None,
    };
    let target_impl = match &args.receiver {
        Receiver::Opaque(opaque) => {
            let methods = methods
                .iter()
                .map(|method| method.imported_method(&args.receiver, &st, true));
            quote! {
                impl #trait_ident for #st::opaque::RefMut<#opaque> {
                    #(#methods)*
                }
            }
        }
        Receiver::Stable(receiver) => {
            let methods = methods
                .iter()
                .map(|method| method.imported_method(&args.receiver, &st, false));
            quote! {
                impl #trait_ident for #receiver {
                    #(#methods)*
                }
            }
        }
    };
    quote! {
        #item
        #imports
        #ref_impl
        #target_impl
    }
}

pub fn interface(attrs: proc_macro::TokenStream, item: ItemTrait) -> proc_macro2::TokenStream {
    let args: InterfaceArgs = syn::parse(attrs).unwrap();
    if args.has_link_args {
        panic!("`stabby::interface` does not link symbols; use `stabby::import_interface` for link-time imports");
    }
    if args.vtable.is_some() {
        panic!("`vtable = Type` is only supported on `stabby::export_interface`");
    }
    let Receiver::Opaque(opaque) = &args.receiver else {
        panic!("runtime bound interfaces currently require `opaque = Type`");
    };
    let st = crate::tl_mod();
    let methods = trait_methods(&item, &args.prefix);
    if args.resolver
        && !methods
            .iter()
            .any(|method| method.ident == "query_interface")
    {
        panic!("`stabby::interface(..., resolver)` requires a `query_interface` method");
    }
    let trait_ident = &item.ident;
    let vtable_ident = format_ident!("{trait_ident}VTable");
    let resolver_ident = format_ident!("{trait_ident}InterfaceResolver");
    let vis = &item.vis;
    let interface_id_fn = symbol_ident(&args.prefix, &format_ident!("interface_id"));
    let interface_report_fn = symbol_ident(&args.prefix, &format_ident!("interface_report"));
    let fields = methods.iter().map(|method| {
        let ident = &method.ident;
        let ty = method.vtable_fn_type(&args.receiver, &st);
        quote!(pub #ident: #ty)
    });
    let constructor_args = methods.iter().map(|method| {
        let ident = &method.ident;
        let ty = method.vtable_fn_type(&args.receiver, &st);
        quote!(#ident: #ty)
    });
    let constructor_fields = methods.iter().map(|method| &method.ident);
    let mut report = crate::Report::r#struct(vtable_ident.to_string(), 0, quote!());
    let mut layout = None;
    for method in &methods {
        let ty = method.vtable_fn_type(&args.receiver, &st);
        layout = Some(layout.map_or_else(
            || quote!(#ty),
            |layout| quote!(#st::FieldPair<#layout, #ty>),
        ));
        report.add_field(method.ident.to_string(), ty);
    }
    let layout = layout.map_or_else(|| quote!(()), |layout| quote!(#st::Struct<#layout>));
    let report_bounds = report.bounds();
    let ctype = cfg!(feature = "experimental-ctypes").then(|| {
        quote! {
            type CType = <#layout as #st::IStable>::CType;
        }
    });
    let ref_impl = if methods.iter().all(|method| !method.receiver_mut) {
        let methods = methods.iter().map(|method| method.bound_method(false));
        Some(quote! {
            impl #trait_ident for #st::opaque::InterfaceRef<#opaque, #vtable_ident> {
                #(#methods)*
            }
        })
    } else {
        None
    };
    let mut_methods = methods.iter().map(|method| method.bound_method(true));
    let resolver_impl = args.resolver.then(|| {
        quote! {
            #[allow(missing_docs)]
            #vis trait #resolver_ident {
                fn resolve_interface<VTable>(&mut self) -> #st::option::Option<#st::opaque::InterfaceRefMut<#opaque, VTable>>
                where
                    VTable: #st::IStable,
                    #st::opaque::InterfaceRefMut<#opaque, VTable>: #st::IStable + #st::IDeterminantProvider<()>;
            }

            impl #resolver_ident for #st::opaque::InterfaceRefMut<#opaque, #vtable_ident> {
                fn resolve_interface<VTable>(&mut self) -> #st::option::Option<#st::opaque::InterfaceRefMut<#opaque, VTable>>
                where
                    VTable: #st::IStable,
                    #st::opaque::InterfaceRefMut<#opaque, VTable>: #st::IStable + #st::IDeterminantProvider<()>,
                {
                    let _stabby_interface = <Self as #trait_ident>::query_interface(
                        self,
                        <VTable as #st::IStable>::ID,
                        <VTable as #st::IStable>::REPORT,
                    );
                    _stabby_interface.match_owned(
                        |_stabby_interface| #st::option::Option::Some(unsafe {
                            _stabby_interface.assume_vtable::<VTable>()
                        }),
                        || #st::option::Option::None(),
                    )
                }
            }
        }
    });
    quote! {
        #item

        #[repr(C)]
        #[derive(Clone, Copy)]
        #[allow(missing_docs)]
        #vis struct #vtable_ident {
            #(#fields,)*
        }

        impl #vtable_ident {
            #[allow(missing_docs)]
            pub const fn new(#(#constructor_args),*) -> Self {
                Self {
                    #(#constructor_fields,)*
                }
            }
        }

        #[automatically_derived]
        // SAFETY: This is generated by `stabby`, and the vtable is a `repr(C)`
        // struct whose report follows its function pointer fields.
        unsafe impl #st::IStable for #vtable_ident where #layout: #st::IStable, #report_bounds {
            type ForbiddenValues = <#layout as #st::IStable>::ForbiddenValues;
            type UnusedBits = <#layout as #st::IStable>::UnusedBits;
            type Size = <#layout as #st::IStable>::Size;
            type Align = <#layout as #st::IStable>::Align;
            type HasExactlyOneNiche = <#layout as #st::IStable>::HasExactlyOneNiche;
            type ContainsIndirections = <#layout as #st::IStable>::ContainsIndirections;
            #ctype
            const REPORT: &'static #st::report::TypeReport = &#report;
            const ID: u64 = #st::report::gen_id(Self::REPORT);
        }

        #[allow(missing_docs)]
        #vis const fn #interface_id_fn() -> u64 {
            <#vtable_ident as #st::IStable>::ID
        }

        #[allow(missing_docs)]
        #vis const fn #interface_report_fn() -> &'static #st::report::TypeReport {
            <#vtable_ident as #st::IStable>::REPORT
        }

        #ref_impl

        impl #trait_ident for #st::opaque::InterfaceRefMut<#opaque, #vtable_ident> {
            #(#mut_methods)*
        }

        #resolver_impl
    }
}

pub fn export_interface(
    attrs: proc_macro::TokenStream,
    item: ItemImpl,
) -> proc_macro2::TokenStream {
    let args: InterfaceArgs = syn::parse(attrs).unwrap();
    let st = crate::tl_mod();
    let Some((_, trait_path, _)) = &item.trait_ else {
        panic!("`stabby::export_interface` must be placed on a trait impl")
    };
    let self_ty = item.self_ty.as_ref();
    let methods = impl_methods(&item, &args.prefix);
    let exports = methods
        .iter()
        .map(|method| method.export_item(&args.receiver, &st, trait_path, self_ty));
    let vtable_items = interface_vtable_items(&args, &methods, &st);
    quote! {
        #item
        #(#exports)*
        #vtable_items
    }
}

pub fn opaque(attrs: proc_macro::TokenStream, item: ItemStruct) -> proc_macro2::TokenStream {
    let OpaqueArgs { version, module } = syn::parse(attrs).unwrap();
    if !matches!(item.fields, syn::Fields::Unit) {
        panic!("`stabby::opaque` must be used on a unit struct marker");
    }
    if !item.generics.params.is_empty() {
        panic!("`stabby::opaque` doesn't support generic markers");
    }
    let st = crate::tl_mod();
    let attrs = &item.attrs;
    let vis = &item.vis;
    let ident = &item.ident;
    let report = crate::Report::r#struct(ident.to_string(), version, module);
    let ctype = cfg!(feature = "experimental-ctypes").then(|| {
        quote!(
            type CType = ();
        )
    });
    quote! {
        #(#attrs)*
        #[repr(C)]
        #vis struct #ident;

        // SAFETY: Opaque markers are ZSTs used only to name an ABI contract.
        unsafe impl #st::IStable for #ident {
            type Size = #st::U0;
            type Align = #st::U1;
            type ForbiddenValues = #st::End;
            type UnusedBits = #st::End;
            type HasExactlyOneNiche = #st::B0;
            type ContainsIndirections = #st::B0;
            #ctype
            const REPORT: &'static #st::report::TypeReport = &#report;
            const ID: u64 = #st::report::gen_id(Self::REPORT);
        }
    }
}
