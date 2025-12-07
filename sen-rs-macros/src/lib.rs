use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, Data, DeriveInput, Fields, ItemFn, Token,
};

/// Derives the SenRouter trait for an enum, generating the `execute()` method.
///
/// # Usage
///
/// ```ignore
/// #[derive(SenRouter)]
/// #[sen(state = AppState)]
/// enum Commands {
///     #[sen(handler = handlers::status)]
///     Status,
///
///     #[sen(handler = handlers::build)]
///     Build(BuildArgs),
/// }
/// ```
///
/// This will generate:
///
/// ```ignore
/// impl Commands {
///     pub async fn execute(self, state: sen::State<AppState>) -> sen::Response {
///         match self {
///             Commands::Status => handlers::status(state).await.into_response(),
///             Commands::Build(args) => handlers::build(state, args).await.into_response(),
///         }
///     }
/// }
/// ```
#[proc_macro_derive(SenRouter, attributes(sen))]
pub fn derive_sen_router(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_name = &input.ident;

    // Extract state type from #[sen(state = AppState)]
    let state_type = extract_state_type(&input.attrs)
        .expect("Missing #[sen(state = YourStateType)] attribute on enum");

    // Generate match arms for each variant
    let match_arms = match &input.data {
        Data::Enum(data) => data
            .variants
            .iter()
            .map(|variant| {
                let variant_name = &variant.ident;
                let handler_path = extract_handler(&variant.attrs)
                    .unwrap_or_else(|| {
                        panic!(
                            "Missing #[sen(handler = path::to::handler)] attribute on variant {}",
                            variant_name
                        )
                    });

                match &variant.fields {
                    Fields::Unit => {
                        // No args, only inject state
                        quote! {
                            #enum_name::#variant_name => {
                                #handler_path(state).await.into_response()
                            }
                        }
                    }
                    Fields::Unnamed(_) => {
                        // Has args, inject state and args
                        quote! {
                            #enum_name::#variant_name(args) => {
                                #handler_path(state, args).await.into_response()
                            }
                        }
                    }
                    Fields::Named(_) => {
                        panic!("Named fields are not supported in SenRouter. Use tuple variants or unit variants.");
                    }
                }
            })
            .collect::<Vec<_>>(),
        _ => panic!("SenRouter can only be derived for enums"),
    };

    // Generate the implementation
    let expanded = quote! {
        impl #enum_name {
            pub async fn execute(self, state: sen::State<#state_type>) -> sen::Response {
                use sen::IntoResponse;

                match self {
                    #(#match_arms)*
                }
            }
        }
    };

    TokenStream::from(expanded)
}

/// Extract the state type from #[sen(state = T)]
fn extract_state_type(attrs: &[syn::Attribute]) -> Option<syn::Type> {
    for attr in attrs {
        if attr.path().is_ident("sen") {
            if let Ok(meta_list) = attr.meta.require_list() {
                // Parse the tokens inside sen(...)
                let tokens = &meta_list.tokens;
                let parsed: Result<syn::MetaNameValue, _> = syn::parse2(tokens.clone());

                if let Ok(nv) = parsed {
                    if nv.path.is_ident("state") {
                        if let syn::Expr::Path(expr_path) = nv.value {
                            return Some(syn::Type::Path(syn::TypePath {
                                qself: None,
                                path: expr_path.path,
                            }));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Extract the handler path from #[sen(handler = path::to::handler)]
fn extract_handler(attrs: &[syn::Attribute]) -> Option<syn::Path> {
    for attr in attrs {
        if attr.path().is_ident("sen") {
            if let Ok(meta_list) = attr.meta.require_list() {
                let tokens = &meta_list.tokens;
                let parsed: Result<syn::MetaNameValue, _> = syn::parse2(tokens.clone());

                if let Ok(nv) = parsed {
                    if nv.path.is_ident("handler") {
                        if let syn::Expr::Path(expr_path) = nv.value {
                            return Some(expr_path.path);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Struct to parse #[sen(...)] attributes
struct SenAttrs {
    name: Option<String>,
    version: Option<String>,
    about: Option<String>,
    desc: Option<String>,
    tier: Option<String>,
    tags: Option<Vec<String>>,
}

impl Parse for SenAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut name = None;
        let mut version = None;
        let mut about = None;
        let mut desc = None;
        let mut tier = None;
        let mut tags = None;

        while !input.is_empty() {
            let ident: syn::Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            match ident.to_string().as_str() {
                "name" => {
                    let value: syn::LitStr = input.parse()?;
                    name = Some(value.value());
                }
                "version" => {
                    let value: syn::LitStr = input.parse()?;
                    version = Some(value.value());
                }
                "about" => {
                    let value: syn::LitStr = input.parse()?;
                    about = Some(value.value());
                }
                "desc" => {
                    let value: syn::LitStr = input.parse()?;
                    desc = Some(value.value());
                }
                "tier" => {
                    let value: syn::LitStr = input.parse()?;
                    tier = Some(value.value());
                }
                "tags" => {
                    // Parse array of strings: ["tag1", "tag2", "tag3"]
                    let content;
                    syn::bracketed!(content in input);
                    let mut tag_list = Vec::new();
                    while !content.is_empty() {
                        let tag: syn::LitStr = content.parse()?;
                        tag_list.push(tag.value());
                        if !content.is_empty() {
                            content.parse::<Token![,]>()?;
                        }
                    }
                    tags = Some(tag_list);
                }
                _ => {
                    // Skip unknown attributes
                    let _: syn::LitStr = input.parse()?;
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(SenAttrs {
            name,
            version,
            about,
            desc,
            tier,
            tags,
        })
    }
}

/// Attribute macro for Router functions to attach CLI metadata.
///
/// # Usage
///
/// ```ignore
/// #[sen(
///     name = "myctl",
///     version = "1.0.0",
///     about = "Cloud Resource Management CLI"
/// )]
/// fn build_router(state: AppState) -> Router<()> {
///     Router::new()
///         .nest("db", db_router)
///         .with_state(state)
/// }
/// ```
///
/// This expands to:
///
/// ```ignore
/// fn build_router(state: AppState) -> Router<()> {
///     let __router = {
///         Router::new()
///             .nest("db", db_router)
///             .with_state(state)
///     };
///     __router.with_metadata(sen::RouterMetadata {
///         name: "myctl",
///         version: Some("1.0.0"),
///         about: Some("Cloud Resource Management CLI"),
///     })
/// }
/// ```
#[proc_macro_attribute]
pub fn sen(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_macro_input!(attr as SenAttrs);

    let name = attrs.name.expect("Missing 'name' attribute in #[sen(...)]");

    let fn_vis = &input.vis;
    let fn_sig = &input.sig;
    let fn_block = &input.block;

    // Build metadata construction
    let version_expr = if let Some(v) = attrs.version {
        quote! { Some(#v) }
    } else {
        quote! { None }
    };

    let about_expr = if let Some(a) = attrs.about {
        quote! { Some(#a) }
    } else {
        quote! { None }
    };

    // Generate the wrapped function
    let expanded = quote! {
        #fn_vis #fn_sig {
            let __router = #fn_block;
            __router.with_metadata(sen::RouterMetadata {
                name: #name,
                version: #version_expr,
                about: #about_expr,
            })
        }
    };

    TokenStream::from(expanded)
}

/// Attribute macro for handler functions to attach metadata.
///
/// Transforms a handler function into a constructor that returns
/// HandlerWithMeta.
///
/// # Usage
///
/// ```ignore
/// #[sen::handler(desc = "Create a new database")]
/// pub async fn create(
///     state: State<AppState>,
///     Args(args): Args<DbCreateArgs>
/// ) -> CliResult<String> {
///     // implementation
/// }
/// ```
///
/// Expands to:
///
/// ```ignore
/// pub fn create() -> HandlerWithMeta<impl Handler<...>, ...> {
///     async fn create_impl(...) -> CliResult<String> { ... }
///     HandlerWithMeta::new(create_impl, HandlerMetadata { desc: Some("...") })
/// }
/// ```
#[proc_macro_attribute]
pub fn handler(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let attrs = parse_macro_input!(attr as SenAttrs);

    let fn_name = &input.sig.ident;
    let fn_vis = &input.vis;
    let fn_block = &input.block;
    let fn_inputs = &input.sig.inputs;
    let fn_output = &input.sig.output;

    // Extract State<S> and Args<T> types from function signature
    let (state_type, args_type) = match extract_handler_types(fn_inputs) {
        Ok(types) => types,
        Err(e) => {
            return syn::Error::new(
                fn_name.span(),
                format!("Failed to extract handler types: {}", e),
            )
            .to_compile_error()
            .into();
        }
    };

    // Create implementation function name
    let impl_name = syn::Ident::new(&format!("{}_impl", fn_name), fn_name.span());

    // Build metadata (prefer desc, fallback to about or name)
    let desc_expr = if let Some(d) = attrs.desc.or(attrs.about.clone()).or(attrs.name.clone()) {
        quote! { Some(#d) }
    } else {
        quote! { None }
    };

    // Build tier expression
    let tier_expr = if let Some(tier_str) = attrs.tier {
        quote! {
            sen::Tier::parse(#tier_str)
        }
    } else {
        quote! { None }
    };

    // Build tags expression
    let tags_expr = if let Some(tag_list) = attrs.tags {
        let tags: Vec<_> = tag_list.iter().collect();
        quote! {
            Some(vec![#(#tags),*])
        }
    } else {
        quote! { None }
    };

    // Generate code with concrete return type
    let expanded = quote! {
        #fn_vis fn #fn_name() -> sen::HandlerWithMeta<
            impl sen::Handler<(sen::State<#state_type>, sen::Args<#args_type>), #state_type>,
            (sen::State<#state_type>, sen::Args<#args_type>),
            #state_type
        > {
            // Implementation function (same signature as original)
            async fn #impl_name(#fn_inputs) #fn_output #fn_block

            // Return wrapped handler with metadata
            sen::HandlerWithMeta::new(
                #impl_name,
                sen::HandlerMetadata {
                    desc: #desc_expr,
                    tier: #tier_expr,
                    tags: #tags_expr,
                }
            )
        }
    };

    TokenStream::from(expanded)
}

/// Extract State<S> and Args<T> types from handler function signature
fn extract_handler_types(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::Token![,]>,
) -> Result<(syn::Type, syn::Type), String> {
    let mut iter = inputs.iter();

    // First parameter should be State<S>
    let state_arg = iter
        .next()
        .ok_or("Handler must have at least one parameter")?;
    let state_type = extract_type_from_state(state_arg)?;

    // Second parameter should be Args<T>
    let args_arg = iter.next().ok_or("Handler must have Args parameter")?;
    let args_type = extract_type_from_args(args_arg)?;

    Ok((state_type, args_type))
}

/// Extract S from State<S> parameter
fn extract_type_from_state(arg: &syn::FnArg) -> Result<syn::Type, String> {
    match arg {
        syn::FnArg::Typed(pat_type) => extract_generic_type(&pat_type.ty, "State"),
        _ => Err("Expected typed parameter".to_string()),
    }
}

/// Extract T from Args(args): Args<T> parameter
fn extract_type_from_args(arg: &syn::FnArg) -> Result<syn::Type, String> {
    match arg {
        syn::FnArg::Typed(pat_type) => extract_generic_type(&pat_type.ty, "Args"),
        _ => Err("Expected typed parameter".to_string()),
    }
}

/// Extract the inner type T from a generic type like State<T> or Args<T>
fn extract_generic_type(ty: &syn::Type, expected_ident: &str) -> Result<syn::Type, String> {
    if let syn::Type::Path(type_path) = ty {
        let last_segment = type_path.path.segments.last().ok_or("Empty type path")?;

        if last_segment.ident != expected_ident {
            return Err(format!("Expected {} type", expected_ident));
        }

        if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
            if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                return Ok(inner_type.clone());
            }
        }
    }

    Err(format!("Could not extract type from {}", expected_ident))
}
