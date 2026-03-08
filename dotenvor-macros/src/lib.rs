use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{
    Attribute, FnArg, Ident, ItemFn, LitBool, LitStr, Meta, Pat, ReturnType, Signature, Token,
    Type,
    parse::{Parse, ParseStream, Parser},
    parse_quote,
    parse_macro_input,
    punctuated::Punctuated,
};

struct LoadArgs {
    path: LitStr,
    required: bool,
    override_existing: bool,
    search_upward: bool,
}

impl Default for LoadArgs {
    fn default() -> Self {
        Self {
            path: LitStr::new(".env", proc_macro2::Span::call_site()),
            required: true,
            override_existing: false,
            search_upward: false,
        }
    }
}

impl Parse for LoadArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = Self::default();
        let mut seen = std::collections::BTreeSet::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let key_name = key.to_string();
            if !seen.insert(key_name.clone()) {
                return Err(syn::Error::new(
                    key.span(),
                    format!("duplicate attribute argument `{key_name}`"),
                ));
            }

            input.parse::<Token![=]>()?;
            match key_name.as_str() {
                "path" => {
                    args.path = input.parse::<LitStr>()?;
                }
                "required" => {
                    args.required = input.parse::<LitBool>()?.value();
                }
                "override_existing" => {
                    args.override_existing = input.parse::<LitBool>()?.value();
                }
                "search_upward" => {
                    args.search_upward = input.parse::<LitBool>()?.value();
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown attribute argument `{key_name}`"),
                    ));
                }
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        Ok(args)
    }
}

/// Load `.env` entries into the process environment before running a function.
///
/// Supported arguments:
/// - `path = "..."` (default: `.env`)
/// - `required = true|false` (default: `true`)
/// - `override_existing = true|false` (default: `false`)
/// - `search_upward = true|false` (default: `false`)
///
/// The annotated function must return `Result<_, _>`.
#[proc_macro_attribute]
pub fn load(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as LoadArgs);
    let function = parse_macro_input!(item as ItemFn);

    match expand_load(args, function) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_load(args: LoadArgs, function: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    ensure_result_return(&function.sig)?;
    let dotenvor_path = resolve_dotenvor_path()?;

    if uses_runtime_entry_wrapper(&function) {
        expand_runtime_wrapper(args, function, &dotenvor_path)
    } else if function.sig.asyncness.is_some() {
        Ok(expand_async_wrapper(args, function, &dotenvor_path))
    } else {
        Ok(expand_inline_prologue(args, function, &dotenvor_path))
    }
}

fn resolve_dotenvor_path() -> syn::Result<proc_macro2::TokenStream> {
    match crate_name("dotenvor") {
        Ok(FoundCrate::Itself) => Ok(quote!(crate)),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, proc_macro2::Span::call_site());
            Ok(quote!(::#ident))
        }
        Err(err) => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("failed to resolve `dotenvor` crate name: {err}"),
        )),
    }
}

fn ensure_result_return(sig: &Signature) -> syn::Result<()> {
    let is_result = match &sig.output {
        ReturnType::Type(_, ty) => is_result_type(ty),
        ReturnType::Default => false,
    };

    if is_result {
        return Ok(());
    }

    Err(syn::Error::new_spanned(
        &sig.output,
        "`#[dotenvor::load]` requires the function to return `Result<_, _>`",
    ))
}

fn is_result_type(ty: &Type) -> bool {
    let Type::Path(path) = ty else {
        return false;
    };

    path.qself.is_none()
        && path
            .path
            .segments
            .last()
            .map(|segment| segment.ident == "Result")
            .unwrap_or(false)
}

fn uses_runtime_entry_wrapper(function: &ItemFn) -> bool {
    function.sig.asyncness.is_some() && function.attrs.iter().any(is_runtime_entry_attr)
}

fn is_runtime_entry_attr(attr: &Attribute) -> bool {
    is_runtime_entry_path(attr.path()) || cfg_attr_contains_runtime_entry(attr)
}

fn is_runtime_entry_path(path: &syn::Path) -> bool {
    let mut segments = path.segments.iter();
    let Some(first) = segments.next() else {
        return false;
    };
    let Some(second) = segments.next() else {
        return false;
    };
    if segments.next().is_some() {
        return false;
    }

    matches!(
        (
            first.ident.to_string().as_str(),
            second.ident.to_string().as_str()
        ),
        ("tokio", "main") | ("async_std", "main") | ("actix_web", "main")
    )
}

fn cfg_attr_contains_runtime_entry(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg_attr") {
        return false;
    }

    parse_cfg_attr_args(attr)
        .map(|args| args.iter().skip(1).any(meta_contains_runtime_entry))
        .unwrap_or(false)
}

fn parse_cfg_attr_args(
    attr: &Attribute,
) -> syn::Result<Punctuated<Meta, Token![,]>> {
    attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
}

fn meta_contains_runtime_entry(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => is_runtime_entry_path(path),
        Meta::List(list) => {
            if is_runtime_entry_path(&list.path) {
                return true;
            }

            if !list.path.is_ident("cfg_attr") {
                return false;
            }

            Punctuated::<Meta, Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .map(|args| args.iter().skip(1).any(meta_contains_runtime_entry))
                .unwrap_or(false)
        }
        Meta::NameValue(value) => is_runtime_entry_path(&value.path),
    }
}

fn is_cfg_attr(attr: &Attribute) -> bool {
    let ident = attr.path().get_ident();
    matches!(ident, Some(name) if name == "cfg" || name == "cfg_attr")
}

fn loader_invocation(
    args: &LoadArgs,
    dotenvor_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let path = &args.path;
    let required = args.required;
    let override_existing = args.override_existing;
    let search_upward = args.search_upward;

    quote! {
        unsafe {
            #dotenvor_path::EnvLoader::new()
                .path(#path)
                .required(#required)
                .override_existing(#override_existing)
                .search_upward(#search_upward)
                .load_and_modify()
        }
    }
}

fn loader_prologue(
    args: &LoadArgs,
    dotenvor_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let invocation = loader_invocation(args, dotenvor_path);

    quote! {
        #invocation?;
    }
}

fn apply_process_env_unsafety(sig: &mut Signature) {
    if sig.ident != "main" && sig.unsafety.is_none() {
        sig.unsafety = Some(Token![unsafe](proc_macro2::Span::call_site()));
    }
}

fn result_return_type(sig: &Signature) -> Type {
    match &sig.output {
        ReturnType::Type(_, ty) => ty.as_ref().clone(),
        ReturnType::Default => unreachable!("load macro validated Result return type"),
    }
}

fn expand_inline_prologue(
    args: LoadArgs,
    function: ItemFn,
    dotenvor_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let prologue = loader_prologue(&args, dotenvor_path);
    let attrs = function.attrs;
    let vis = function.vis;
    let mut sig = function.sig;
    apply_process_env_unsafety(&mut sig);
    let stmts = function.block.stmts;

    quote! {
        #(#attrs)*
        #vis #sig {
            #prologue
            #(#stmts)*
        }
    }
}

fn expand_async_wrapper(
    args: LoadArgs,
    function: ItemFn,
    dotenvor_path: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let load = loader_invocation(&args, dotenvor_path);
    let attrs = function.attrs;
    let vis = function.vis;
    let mut sig = function.sig;
    let output = result_return_type(&sig);
    apply_process_env_unsafety(&mut sig);
    sig.asyncness = None;
    sig.output = parse_quote!(-> impl ::core::future::Future<Output = #output>);
    let block = function.block;

    quote! {
        #(#attrs)*
        #vis #sig {
            let __dotenvor_load_result = #load;

            async move {
                __dotenvor_load_result?;
                #block
            }
        }
    }
}

fn expand_runtime_wrapper(
    args: LoadArgs,
    function: ItemFn,
    dotenvor_path: &proc_macro2::TokenStream,
) -> syn::Result<proc_macro2::TokenStream> {
    let prologue = loader_prologue(&args, dotenvor_path);
    let vis = function.vis;
    let sig = function.sig;
    let block = function.block;

    let (runtime_attrs, passthrough_attrs): (Vec<_>, Vec<_>) =
        function.attrs.into_iter().partition(is_runtime_entry_attr);

    let cfg_attrs: Vec<_> = passthrough_attrs
        .iter()
        .filter(|attr| is_cfg_attr(attr))
        .cloned()
        .collect();

    let fn_name = &sig.ident;
    let inner_name = format_ident!("__dotenvor_load_inner_{fn_name}");

    let mut wrapper_sig = sig.clone();
    wrapper_sig.asyncness = None;
    apply_process_env_unsafety(&mut wrapper_sig);

    let mut inner_sig = sig;
    inner_sig.ident = inner_name.clone();

    let call_args = collect_call_args(&wrapper_sig.inputs)?;
    let (_, inner_ty_generics, _) = wrapper_sig.generics.split_for_impl();
    let inner_turbofish = inner_ty_generics.as_turbofish();
    let inner_invocation = quote!(#inner_name #inner_turbofish (#(#call_args),*));
    let inner_call = if inner_sig.unsafety.is_some() {
        quote!(unsafe { #inner_invocation })
    } else {
        inner_invocation
    };

    Ok(quote! {
        #(#passthrough_attrs)*
        #vis #wrapper_sig {
            #prologue
            #inner_call
        }

        #(#cfg_attrs)*
        #(#runtime_attrs)*
        #vis #inner_sig #block
    })
}

fn collect_call_args(
    inputs: &Punctuated<FnArg, Token![,]>,
) -> syn::Result<Vec<proc_macro2::TokenStream>> {
    let mut args = Vec::with_capacity(inputs.len());

    for input in inputs {
        match input {
            FnArg::Receiver(_) => args.push(quote!(self)),
            FnArg::Typed(typed) => {
                let Pat::Ident(ident) = typed.pat.as_ref() else {
                    return Err(syn::Error::new_spanned(
                        &typed.pat,
                        "arguments must use identifier patterns when combined with async runtime entry macros",
                    ));
                };
                let name = &ident.ident;
                args.push(quote!(#name));
            }
        }
    }

    Ok(args)
}
