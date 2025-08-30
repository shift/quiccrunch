use proc_macro2::TokenStream;
use quote::{quote, format_ident};
use syn::{
    punctuated::Punctuated, Meta, Token, ItemTrait,
    Result as SynResult
};

/// Configuration for the rpc_service macro
#[derive(Debug, Clone, Default)]
struct RpcServiceArgs {
    name: Option<String>,
}

/// Parse simple macro arguments (for now, just service name)
fn parse_args(args: Punctuated<Meta, Token![,]>) -> SynResult<RpcServiceArgs> {
    let mut config = RpcServiceArgs::default();
    
    for meta in args {
        match meta {
            Meta::NameValue(nv) if nv.path.is_ident("name") => {
                if let syn::Expr::Lit(syn::ExprLit { 
                    lit: syn::Lit::Str(s), .. 
                }) = nv.value {
                    config.name = Some(s.value());
                }
            }
            _ => {} // Ignore other arguments for now
        }
    }
    
    Ok(config)
}

/// Main entry point for the rpc_service macro expansion
pub fn expand_rpc_service(args: Punctuated<Meta, Token![,]>, input: ItemTrait) -> SynResult<TokenStream> {
    let args = parse_args(args)?;
    let service_name = args.name.unwrap_or_else(|| input.ident.to_string());
    
    // For now, generate a simple service handler structure
    let trait_ident = &input.ident;
    let handler_ident = format_ident!("{}Handler", service_name);
    let client_ident = format_ident!("{}Client", service_name);
    
    Ok(quote! {
        #input
        
        /// Generated service handler for #trait_ident
        pub struct #handler_ident<S> {
            service: S,
        }
        
        impl<S> #handler_ident<S>
        where
            S: #trait_ident + Send + Sync + Clone + 'static,
        {
            pub fn new(service: S) -> Self {
                Self { service }
            }
        }
        
        /// Generated client for #trait_ident service
        pub struct #client_ident {
            // TODO: Add RPC client field once core integration is ready
        }
        
        impl #client_ident {
            pub fn new() -> Self {
                Self {}
            }
        }
    })
}