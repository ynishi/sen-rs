use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

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
