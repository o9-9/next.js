use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    Attribute, Data, DeriveInput, Expr, ExprLit, Fields, Lit, Token, Variant, parse_macro_input,
    punctuated::Punctuated,
};

/// The parsed form of a `#[value_to_string(...)]` attribute.
enum AttrForm {
    /// `#[value_to_string("{field} text")]` — format string with auto-field references via
    /// `ValueToStringify`.
    FormatAutoFields(String),
    /// `#[value_to_string("fmt {}", expr1, expr2)]` — format string with explicit expression
    /// arguments. Each expression is resolved through `ValueToStringify`.
    FormatExprs(String, Vec<Expr>),
    /// `#[value_to_string(expr)]` — single expression delegation. The expression is resolved
    /// through `ValueToStringify`.
    DirectExpr(Expr),
}

/// Derive macro for `ValueToString`.
///
/// # Usage
///
/// ## Structs with `Display` (no attribute)
/// Delegates to `Display::to_string(self)`:
/// ```ignore
/// #[derive(ValueToString)]
/// struct ModuleId(String); // requires Display impl
/// ```
///
/// ## Format string with auto-field references
/// Field names in `{...}` are automatically prefixed with `self.` and resolved through
/// `ValueToStringify`, which handles both `Display` types and `Vc<T>`/`ResolvedVc<T>`.
/// ```ignore
/// #[derive(ValueToString)]
/// #[value_to_string("{name} ({id})")]
/// struct Foo {
///     name: String,
///     id: u32,
/// }
/// ```
///
/// ## Format string with expression arguments
/// Positional `{}` slots are filled by the expression arguments. Each expression is resolved
/// through `ValueToStringify`.
/// ```ignore
/// #[derive(ValueToString)]
/// #[value_to_string("tsconfig extends {}", self.config.ident())]
/// struct TsExtendsReference {
///     config: ResolvedVc<Box<dyn Source>>,
/// }
/// ```
///
/// ## Direct expression delegation
/// A single expression whose result is resolved through `ValueToStringify`.
/// ```ignore
/// #[derive(ValueToString)]
/// #[value_to_string(self.inner.name)]
/// struct DiskFileSystem { inner: Inner }
/// ```
///
/// ## Enums
/// Each variant can have a `#[value_to_string(...)]` attribute using any of the above forms.
/// Variants without the attribute default to their name.
/// ```ignore
/// #[derive(ValueToString)]
/// enum Kind {
///     #[value_to_string("module")]
///     Module,
///     #[value_to_string("asset {path}")]
///     Asset { path: Vc<FileSystemPath> },
///     #[value_to_string(more)]
///     More(ResolvedVc<Kind>),
/// }
/// ```
pub fn derive_value_to_string(input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);
    let ident = &derive_input.ident;

    match &derive_input.data {
        Data::Struct(data) => {
            let attr = find_attr(&derive_input.attrs);
            generate_struct_impl(ident, &data.fields, attr)
        }
        Data::Enum(data) => generate_enum_impl(ident, &data.variants),
        Data::Union(_) => {
            syn::Error::new_spanned(&derive_input, "ValueToString cannot be derived for unions")
                .to_compile_error()
                .into()
        }
    }
}

/// Look for `#[value_to_string(...)]` attribute and parse it into an `AttrForm`.
fn find_attr(attrs: &[Attribute]) -> Option<AttrForm> {
    for attr in attrs {
        if attr.path().is_ident("value_to_string") {
            match parse_attr(attr) {
                Ok(form) => return Some(form),
                Err(e) => {
                    e.span()
                        .unwrap()
                        .error(format!("invalid value_to_string attribute: {e}"))
                        .emit();
                    return None;
                }
            }
        }
    }
    None
}

/// Parse a `#[value_to_string(...)]` attribute into an `AttrForm`.
fn parse_attr(attr: &Attribute) -> syn::Result<AttrForm> {
    let args: Punctuated<Expr, Token![,]> = attr.parse_args_with(Punctuated::parse_terminated)?;
    let mut iter = args.into_iter();

    let first = iter
        .next()
        .ok_or_else(|| syn::Error::new_spanned(attr, "expected format string or expression"))?;

    // Check if first arg is a string literal
    if let Expr::Lit(ExprLit {
        lit: Lit::Str(s), ..
    }) = &first
    {
        let fmt = s.value();
        let rest: Vec<Expr> = iter.collect();
        if rest.is_empty() {
            Ok(AttrForm::FormatAutoFields(fmt))
        } else {
            Ok(AttrForm::FormatExprs(fmt, rest))
        }
    } else {
        // Single expression — no additional args allowed
        if let Some(extra) = iter.next() {
            return Err(syn::Error::new_spanned(
                extra,
                "expected format string as first argument when providing multiple arguments",
            ));
        }
        Ok(AttrForm::DirectExpr(first))
    }
}

/// Returns true if the format string is a pure constant (no `{` or `}` characters),
/// meaning it can be used with `rcstr!` directly instead of `format!`.
fn is_pure_constant(fmt: &str) -> bool {
    !fmt.contains('{') && !fmt.contains('}')
}

// ---------------------------------------------------------------------------
// Format string parsing (for auto-field mode)
// ---------------------------------------------------------------------------

/// Extract field references from a format string like `"{name} ({id})"`.
///
/// Returns:
/// - The transformed format string (with `{0}` → `{_0}` for positional fields)
/// - A deduplicated list of field name strings in order of first appearance
fn parse_format_fields(fmt: &str) -> (String, Vec<String>) {
    let mut fields = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut transformed = String::new();

    let chars: Vec<char> = fmt.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                // Escaped brace
                transformed.push_str("{{");
                i += 2;
                continue;
            }
            // Start of field reference
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '}' {
                i += 1;
            }
            let field_name: String = chars[start..i].iter().collect();
            i += 1; // skip closing '}'

            // For numeric field names, prefix with _ for the local variable
            let var_name = if field_name.chars().all(|c| c.is_ascii_digit()) {
                format!("_{field_name}")
            } else {
                field_name.clone()
            };

            if seen.insert(field_name.clone()) {
                fields.push(field_name);
            }

            transformed.push('{');
            transformed.push_str(&var_name);
            transformed.push('}');
        } else if chars[i] == '}' && i + 1 < chars.len() && chars[i + 1] == '}' {
            // Escaped closing brace
            transformed.push_str("}}");
            i += 2;
        } else {
            transformed.push(chars[i]);
            i += 1;
        }
    }

    (transformed, fields)
}

// ---------------------------------------------------------------------------
// Field access helpers
// ---------------------------------------------------------------------------

/// Generate the field access token stream for a given field name on `self`.
/// Named fields use `self.name`, positional fields use `self.0`.
fn struct_field_access(field_name: &str) -> TokenStream2 {
    if field_name.chars().all(|c| c.is_ascii_digit()) {
        let idx = syn::Index::from(field_name.parse::<usize>().unwrap());
        quote! { self.#idx }
    } else {
        let field_ident = format_ident!("{}", field_name);
        quote! { self.#field_ident }
    }
}

/// Generate a resolve statement: `let var = ValueToStringify::to_stringify(access).await?;`
///
/// When `add_ref` is true (struct context), an extra `&` is added because `self.field` gives
/// owned values. When false (enum context), no `&` is added because destructured bindings in
/// match arms are already references (e.g., `_0: &ResolvedVc<T>`), and adding `&` would create
/// `&&ResolvedVc<T>` which doesn't satisfy `ValueToStringify`.
fn generate_resolve(var_name: &syn::Ident, access: &TokenStream2, add_ref: bool) -> TokenStream2 {
    if add_ref {
        quote! {
            let #var_name = turbo_tasks::display::ValueToStringify::to_stringify(&(#access)).await?;
        }
    } else {
        quote! {
            let #var_name = turbo_tasks::display::ValueToStringify::to_stringify(#access).await?;
        }
    }
}

/// Generate a variable name for a field. Numeric names get a `_` prefix.
fn field_var_name(field_name: &str) -> syn::Ident {
    if field_name.chars().all(|c| c.is_ascii_digit()) {
        format_ident!("_{}", field_name)
    } else {
        format_ident!("{}", field_name)
    }
}

// ---------------------------------------------------------------------------
// Struct code generation
// ---------------------------------------------------------------------------

fn generate_struct_impl(
    ident: &syn::Ident,
    fields: &Fields,
    attr: Option<AttrForm>,
) -> TokenStream {
    match attr {
        Some(AttrForm::FormatAutoFields(fmt)) => generate_struct_format_auto_fields(ident, &fmt),
        Some(AttrForm::FormatExprs(fmt, exprs)) => {
            generate_struct_format_exprs(ident, &fmt, &exprs)
        }
        Some(AttrForm::DirectExpr(expr)) => generate_struct_direct_expr(ident, &expr),
        None => {
            // Delegate to Display
            let _ = fields;
            quote! {
                #[turbo_tasks::value_impl]
                impl turbo_tasks::ValueToString for #ident {
                    #[turbo_tasks::function]
                    fn to_string(&self) -> turbo_tasks::Vc<turbo_rcstr::RcStr> {
                        turbo_tasks::Vc::cell(self.to_string().into())
                    }
                }
            }
            .into()
        }
    }
}

/// Format string with auto-field references: `#[value_to_string("{field}")]`
fn generate_struct_format_auto_fields(ident: &syn::Ident, fmt: &str) -> TokenStream {
    let (transformed_fmt, field_refs) = parse_format_fields(fmt);

    if field_refs.is_empty() {
        // No field references — sync function, use rcstr! for pure constants
        let value_expr = if is_pure_constant(fmt) {
            quote! { turbo_rcstr::rcstr!(#transformed_fmt) }
        } else {
            quote! { format!(#transformed_fmt).into() }
        };
        return quote! {
            #[turbo_tasks::value_impl]
            impl turbo_tasks::ValueToString for #ident {
                #[turbo_tasks::function]
                fn to_string(&self) -> turbo_tasks::Vc<turbo_rcstr::RcStr> {
                    turbo_tasks::Vc::cell(#value_expr)
                }
            }
        }
        .into();
    }

    let resolves: Vec<TokenStream2> = field_refs
        .iter()
        .map(|field_name| {
            let access = struct_field_access(field_name);
            let var = field_var_name(field_name);
            generate_resolve(&var, &access, true)
        })
        .collect();

    quote! {
        #[turbo_tasks::value_impl]
        impl turbo_tasks::ValueToString for #ident {
            #[turbo_tasks::function]
            async fn to_string(&self) -> anyhow::Result<turbo_tasks::Vc<turbo_rcstr::RcStr>> {
                #(#resolves)*
                Ok(turbo_tasks::Vc::cell(format!(#transformed_fmt).into()))
            }
        }
    }
    .into()
}

/// Format string with expression arguments: `#[value_to_string("fmt {}", expr1)]`
fn generate_struct_format_exprs(ident: &syn::Ident, fmt: &str, exprs: &[Expr]) -> TokenStream {
    let resolve_stmts: Vec<TokenStream2> = exprs
        .iter()
        .enumerate()
        .map(|(i, expr)| {
            let var = format_ident!("__arg{}", i);
            quote! {
                let #var = turbo_tasks::display::ValueToStringify::to_stringify(&(#expr)).await?;
            }
        })
        .collect();

    let vars: Vec<syn::Ident> = (0..exprs.len())
        .map(|i| format_ident!("__arg{}", i))
        .collect();

    quote! {
        #[turbo_tasks::value_impl]
        impl turbo_tasks::ValueToString for #ident {
            #[turbo_tasks::function]
            async fn to_string(&self) -> anyhow::Result<turbo_tasks::Vc<turbo_rcstr::RcStr>> {
                #(#resolve_stmts)*
                Ok(turbo_tasks::Vc::cell(format!(#fmt, #(#vars),*).into()))
            }
        }
    }
    .into()
}

/// Direct expression delegation: `#[value_to_string(expr)]`
fn generate_struct_direct_expr(ident: &syn::Ident, expr: &Expr) -> TokenStream {
    quote! {
        #[turbo_tasks::value_impl]
        impl turbo_tasks::ValueToString for #ident {
            #[turbo_tasks::function]
            async fn to_string(&self) -> anyhow::Result<turbo_tasks::Vc<turbo_rcstr::RcStr>> {
                let __val = turbo_tasks::display::ValueToStringify::to_stringify(&(#expr)).await?;
                Ok(turbo_tasks::Vc::cell(__val.into()))
            }
        }
    }
    .into()
}

// ---------------------------------------------------------------------------
// Enum code generation
// ---------------------------------------------------------------------------

fn generate_enum_impl(
    ident: &syn::Ident,
    variants: &Punctuated<Variant, syn::Token![,]>,
) -> TokenStream {
    let mut match_arms = Vec::new();
    let mut needs_async = false;

    for variant in variants {
        let variant_ident = &variant.ident;
        let attr = find_attr(&variant.attrs);

        match attr {
            Some(AttrForm::FormatExprs(fmt, exprs)) => {
                needs_async = true;
                let arm =
                    generate_enum_format_exprs(ident, variant_ident, &variant.fields, &fmt, &exprs);
                match_arms.push(arm);
            }
            Some(AttrForm::DirectExpr(expr)) => {
                needs_async = true;
                let arm = generate_enum_direct_expr(ident, variant_ident, &variant.fields, &expr);
                match_arms.push(arm);
            }
            Some(AttrForm::FormatAutoFields(fmt)) => {
                let arm = generate_enum_format_auto_fields(
                    ident,
                    variant_ident,
                    &variant.fields,
                    &fmt,
                    &mut needs_async,
                );
                match_arms.push(arm);
            }
            None => {
                // Default: use variant name as the string
                let name = variant_ident.to_string();
                let arm = generate_enum_format_auto_fields(
                    ident,
                    variant_ident,
                    &variant.fields,
                    &name,
                    &mut needs_async,
                );
                match_arms.push(arm);
            }
        }
    }

    if needs_async {
        quote! {
            #[turbo_tasks::value_impl]
            impl turbo_tasks::ValueToString for #ident {
                #[turbo_tasks::function]
                async fn to_string(&self) -> anyhow::Result<turbo_tasks::Vc<turbo_rcstr::RcStr>> {
                    let s = match self {
                        #(#match_arms)*
                    };
                    Ok(turbo_tasks::Vc::cell(s.into()))
                }
            }
        }
        .into()
    } else {
        quote! {
            #[turbo_tasks::value_impl]
            impl turbo_tasks::ValueToString for #ident {
                #[turbo_tasks::function]
                fn to_string(&self) -> turbo_tasks::Vc<turbo_rcstr::RcStr> {
                    let s = match self {
                        #(#match_arms)*
                    };
                    turbo_tasks::Vc::cell(s.into())
                }
            }
        }
        .into()
    }
}

/// Generate match arm for a variant with auto-field format string.
fn generate_enum_format_auto_fields(
    ident: &syn::Ident,
    variant_ident: &syn::Ident,
    fields: &Fields,
    fmt: &str,
    needs_async: &mut bool,
) -> TokenStream2 {
    let (transformed_fmt, field_refs) = parse_format_fields(fmt);

    if !field_refs.is_empty() {
        *needs_async = true;
    }

    // For constant strings (no field refs), use rcstr! for pure constants
    let value_expr = if field_refs.is_empty() && is_pure_constant(fmt) {
        quote! { turbo_rcstr::rcstr!(#transformed_fmt) }
    } else {
        quote! { turbo_rcstr::RcStr::from(format!(#transformed_fmt)) }
    };

    match fields {
        Fields::Named(named) => {
            let field_patterns: Vec<TokenStream2> = named
                .named
                .iter()
                .map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    if field_refs.iter().any(|r| r == &name.to_string()) {
                        quote! { #name }
                    } else {
                        quote! { #name: _ }
                    }
                })
                .collect();

            let resolves: Vec<TokenStream2> = field_refs
                .iter()
                .map(|field_name| {
                    let field_ident = format_ident!("{}", field_name);
                    let var = field_var_name(field_name);
                    generate_resolve(&var, &quote! { #field_ident }, false)
                })
                .collect();

            quote! {
                #ident::#variant_ident { #(#field_patterns),* } => {
                    #(#resolves)*
                    #value_expr
                }
            }
        }
        Fields::Unnamed(unnamed) => {
            let field_patterns: Vec<TokenStream2> = (0..unnamed.unnamed.len())
                .map(|i| {
                    let idx_str = i.to_string();
                    if field_refs.iter().any(|r| r == &idx_str) {
                        let var = format_ident!("_{}", i);
                        quote! { #var }
                    } else {
                        quote! { _ }
                    }
                })
                .collect();

            let resolves: Vec<TokenStream2> = field_refs
                .iter()
                .map(|field_name| {
                    let var = field_var_name(field_name);
                    generate_resolve(&var, &quote! { #var }, false)
                })
                .collect();

            quote! {
                #ident::#variant_ident(#(#field_patterns),*) => {
                    #(#resolves)*
                    #value_expr
                }
            }
        }
        Fields::Unit => {
            quote! {
                #ident::#variant_ident => {
                    #value_expr
                }
            }
        }
    }
}

/// Generate match arm for a variant with format + expression args.
fn generate_enum_format_exprs(
    ident: &syn::Ident,
    variant_ident: &syn::Ident,
    fields: &Fields,
    fmt: &str,
    exprs: &[Expr],
) -> TokenStream2 {
    let pattern = enum_destructure_all(ident, variant_ident, fields);

    let resolve_stmts: Vec<TokenStream2> = exprs
        .iter()
        .enumerate()
        .map(|(i, expr)| {
            let var = format_ident!("__arg{}", i);
            quote! {
                let #var = turbo_tasks::display::ValueToStringify::to_stringify(#expr).await?;
            }
        })
        .collect();

    let vars: Vec<syn::Ident> = (0..exprs.len())
        .map(|i| format_ident!("__arg{}", i))
        .collect();

    quote! {
        #pattern => {
            #(#resolve_stmts)*
            turbo_rcstr::RcStr::from(format!(#fmt, #(#vars),*))
        }
    }
}

/// Generate match arm for a variant with a direct expression.
fn generate_enum_direct_expr(
    ident: &syn::Ident,
    variant_ident: &syn::Ident,
    fields: &Fields,
    expr: &Expr,
) -> TokenStream2 {
    let pattern = enum_destructure_all(ident, variant_ident, fields);

    quote! {
        #pattern => {
            turbo_rcstr::RcStr::from(turbo_tasks::display::ValueToStringify::to_stringify(#expr).await?)
        }
    }
}

/// Generate a destructuring pattern that binds ALL fields of an enum variant.
/// Named fields bind to their name; tuple fields bind to `_0`, `_1`, etc.
fn enum_destructure_all(
    ident: &syn::Ident,
    variant_ident: &syn::Ident,
    fields: &Fields,
) -> TokenStream2 {
    match fields {
        Fields::Named(named) => {
            let bindings: Vec<TokenStream2> = named
                .named
                .iter()
                .map(|f| {
                    let name = f.ident.as_ref().unwrap();
                    quote! { #name }
                })
                .collect();
            quote! { #ident::#variant_ident { #(#bindings),* } }
        }
        Fields::Unnamed(unnamed) => {
            let bindings: Vec<TokenStream2> = (0..unnamed.unnamed.len())
                .map(|i| {
                    let var = format_ident!("_{}", i);
                    quote! { #var }
                })
                .collect();
            quote! { #ident::#variant_ident(#(#bindings),*) }
        }
        Fields::Unit => {
            quote! { #ident::#variant_ident }
        }
    }
}
