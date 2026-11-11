use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    punctuated::Punctuated, Data, DeriveInput, Error as SynError, Meta, Result as SynResult, Token,
};

/// Configuration for the rpc_client macro
#[derive(Debug, Clone, Default)]
struct RpcClientArgs {
    service: Option<String>,
}

/// Parse simple macro arguments
fn parse_args(args: Punctuated<Meta, Token![,]>) -> SynResult<RpcClientArgs> {
    let mut config = RpcClientArgs::default();

    for meta in args {
        match meta {
            Meta::NameValue(nv) if nv.path.is_ident("service") => {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = nv.value
                {
                    config.service = Some(s.value());
                }
            }
            _ => {} // Ignore other arguments for now
        }
    }

    Ok(config)
}

/// Main entry point for the rpc_client macro expansion
pub fn expand_rpc_client(
    args: Punctuated<Meta, Token![,]>,
    input: DeriveInput,
) -> SynResult<TokenStream> {
    let args = parse_args(args)?;
    let _service_name = args.service.unwrap_or_else(|| "Unknown".to_string());

    // Validate that this is a struct
    let _struct_data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Err(SynError::new_spanned(
                input,
                "rpc_client can only be applied to structs",
            ))
        }
    };

    let client_ident = &input.ident;

    Ok(quote! {
        #input

        impl #client_ident {
            /// Create a new client instance
            pub fn new() -> Self {
                Self {}
            }
        }

        impl Default for #client_ident {
            fn default() -> Self {
                Self::new()
            }
        }
    })
}
