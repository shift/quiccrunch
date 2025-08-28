/// Utility functions for macro implementations
use syn::{Type, TypePath, PathSegment, GenericArgument, AngleBracketedGenericArguments};

/// Extract the inner type from a generic type like Option<T> or Result<T, E>
pub fn extract_generic_inner_type(ty: &Type, expected_outer: &str) -> Option<Type> {
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(PathSegment { ident, arguments, .. }) = path.segments.last() {
            if ident == expected_outer {
                if let syn::PathArguments::AngleBracketed(AngleBracketedGenericArguments { args, .. }) = arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.first() {
                        return Some(inner_ty.clone());
                    }
                }
            }
        }
    }
    None
}

/// Check if a type is an async return type (Future)
pub fn is_async_return_type(ty: &Type) -> bool {
    // This is a simplified check - in practice you'd want more sophisticated analysis
    match ty {
        Type::ImplTrait(_) => true, // impl Future<...>
        Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                segment.ident == "Future" || segment.ident == "Pin"
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Extract method name from an identifier, converting to snake_case if needed
pub fn normalize_method_name(name: &str) -> String {
    // For now, just return as-is. In practice, you might want to convert
    // CamelCase to snake_case for consistency
    name.to_string()
}

/// Generate a qualified type path for QuicCrunch types
pub fn quiccrunch_type(type_name: &str) -> syn::Type {
    syn::parse_str(&format!("quiccrunch::{}", type_name))
        .unwrap_or_else(|_| syn::parse_quote!(()))
}

/// Check if a type represents a streaming parameter
pub fn is_streaming_param_type(ty: &Type, expected_type: &str) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == expected_type;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_extract_generic_inner_type() {
        let result_type: Type = parse_quote!(Result<i32, String>);
        let inner = extract_generic_inner_type(&result_type, "Result");
        assert!(inner.is_some());
        
        let option_type: Type = parse_quote!(Option<String>);
        let inner = extract_generic_inner_type(&option_type, "Option");
        assert!(inner.is_some());
    }

    #[test]
    fn test_normalize_method_name() {
        assert_eq!(normalize_method_name("testMethod"), "testMethod");
        assert_eq!(normalize_method_name("test_method"), "test_method");
    }
}