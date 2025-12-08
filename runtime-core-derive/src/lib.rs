//! Proc-macros for RemoteMedia runtime-core
//!
//! Provides two attribute macros:
//!
//! ## `#[node_config]` - Config struct generation
//!
//! For defining node configuration structs with automatic TypeScript type generation.
//!
//! ```ignore
//! #[node_config(
//!     node_type = "SpeculativeVADGate",
//!     category = "audio",
//!     description = "Speculative VAD gate for low-latency voice interaction"
//! )]
//! pub struct SpeculativeVADConfig {
//!     pub lookback_ms: u32,
//!     pub sample_rate: u32,
//! }
//! ```
//!
//! ## `#[node]` - Unified node definition (NEW)
//!
//! For defining complete streaming nodes with minimal boilerplate. Combines config struct
//! generation, `AsyncStreamingNode` trait implementation, and TypeScript types into one
//! declarative definition.
//!
//! ```ignore
//! #[node(
//!     node_type = "Echo",
//!     category = "utility",
//!     description = "Echoes input with optional prefix",
//!     accepts = "text",
//!     produces = "text"
//! )]
//! pub struct EchoNode {
//!     #[config(default = "Echo: ".to_string())]
//!     pub prefix: String,
//!
//!     #[state]
//!     call_count: u64,
//! }
//!
//! impl EchoNode {
//!     async fn process(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
//!         // Your implementation here
//!     }
//! }
//! ```
//!
//! This automatically:
//! 1. Generates `EchoNodeConfig` struct from `#[config]` fields
//! 2. Derives `Debug`, `Clone`, `Serialize`, `Deserialize`, `JsonSchema` on config
//! 3. Adds `#[serde(default, rename_all = "camelCase")]` to config
//! 4. Rewrites `EchoNode` to hold `config: EchoNodeConfig` + state fields
//! 5. Generates `new(config)` and `with_default()` constructors
//! 6. Implements `AsyncStreamingNode` trait with delegation to user's `process` method
//! 7. Registers schema for TypeScript generation via `inventory`

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Meta, NestedMeta, Lit};
use syn::parse::Parser;

// =============================================================================
// #[node] macro - Unified node definition
// =============================================================================

/// Arguments parsed from `#[node(...)]` attribute
struct NodeArgs {
    /// Node type identifier (defaults to struct name minus "Node" suffix)
    node_type: Option<String>,
    /// Category for grouping ("audio", "video", "ml", "text", "utility")
    category: Option<String>,
    /// Human-readable description
    description: Option<String>,
    /// Accepted input RuntimeData types (comma-separated)
    accepts: Option<String>,
    /// Produced output RuntimeData types (comma-separated)
    produces: Option<String>,
    /// Whether this node produces multiple outputs per input
    multi_output: bool,
}

/// Classification of a field as either config or state
#[derive(Debug, Clone)]
enum FieldKind {
    /// Config field - included in generated {NodeName}Config struct
    Config {
        /// Optional default value expression
        default_expr: Option<syn::Expr>,
    },
    /// State field - kept in node struct, excluded from config
    State {
        /// Optional default value expression
        default_expr: Option<syn::Expr>,
    },
}

/// A field that has been classified with its kind
struct ClassifiedField {
    /// The original field
    field: syn::Field,
    /// The classification (config or state)
    kind: FieldKind,
}

/// Parse `#[node(...)]` attribute arguments
fn parse_node_args(args: &[NestedMeta]) -> Result<NodeArgs, syn::Error> {
    let mut node_type = None;
    let mut category = None;
    let mut description = None;
    let mut accepts = None;
    let mut produces = None;
    let mut multi_output = false;

    for arg in args {
        match arg {
            NestedMeta::Meta(Meta::NameValue(nv)) => {
                let ident = nv.path.get_ident()
                    .ok_or_else(|| syn::Error::new_spanned(&nv.path, "expected identifier"))?;

                match ident.to_string().as_str() {
                    "node_type" => {
                        if let Lit::Str(s) = &nv.lit {
                            node_type = Some(s.value());
                        }
                    }
                    "category" => {
                        if let Lit::Str(s) = &nv.lit {
                            category = Some(s.value());
                        }
                    }
                    "description" => {
                        if let Lit::Str(s) = &nv.lit {
                            description = Some(s.value());
                        }
                    }
                    "accepts" => {
                        if let Lit::Str(s) = &nv.lit {
                            accepts = Some(s.value());
                        }
                    }
                    "produces" => {
                        if let Lit::Str(s) = &nv.lit {
                            produces = Some(s.value());
                        }
                    }
                    "multi_output" => {
                        if let Lit::Bool(b) = &nv.lit {
                            multi_output = b.value;
                        }
                    }
                    other => {
                        return Err(syn::Error::new_spanned(&nv.path, format!("unknown attribute: {}", other)));
                    }
                }
            }
            NestedMeta::Meta(Meta::Path(path)) => {
                if path.is_ident("multi_output") {
                    multi_output = true;
                }
            }
            _ => {}
        }
    }

    Ok(NodeArgs {
        node_type,
        category,
        description,
        accepts,
        produces,
        multi_output,
    })
}

/// Parse `#[config]` or `#[config(default = expr)]` attribute on a field
fn parse_config_attr(attr: &syn::Attribute) -> Result<Option<syn::Expr>, syn::Error> {
    // Check if it's just `#[config]` with no arguments
    if attr.tokens.is_empty() {
        return Ok(None);
    }

    // Try to parse as a list with arbitrary expression: #[config(default = expr)]
    // We need custom parsing since syn's Meta::NameValue only accepts literals
    let parser = |input: syn::parse::ParseStream| -> syn::Result<Option<syn::Expr>> {
        let content;
        syn::parenthesized!(content in input);

        // Look for "default = expr"
        let ident: syn::Ident = content.parse()?;
        if ident != "default" {
            return Err(syn::Error::new(ident.span(), "expected 'default'"));
        }
        content.parse::<syn::Token![=]>()?;
        let expr: syn::Expr = content.parse()?;
        Ok(Some(expr))
    };

    match parser.parse2(attr.tokens.clone()) {
        Ok(expr) => Ok(expr),
        Err(_) => {
            // Fall back to Meta parsing for simple literals
            let meta = attr.parse_meta()?;
            match meta {
                Meta::Path(_) => Ok(None),
                Meta::List(list) => {
                    for nested in &list.nested {
                        if let NestedMeta::Meta(Meta::NameValue(nv)) = nested {
                            if nv.path.is_ident("default") {
                                let expr = lit_to_expr(&nv.lit)?;
                                return Ok(Some(expr));
                            }
                        }
                    }
                    Ok(None)
                }
                Meta::NameValue(_) => {
                    Err(syn::Error::new_spanned(attr, "expected #[config] or #[config(default = expr)]"))
                }
            }
        }
    }
}

/// Parse `#[state]` or `#[state(default = expr)]` attribute on a field
fn parse_state_attr(attr: &syn::Attribute) -> Result<Option<syn::Expr>, syn::Error> {
    // Check if it's just `#[state]` with no arguments
    if attr.tokens.is_empty() {
        return Ok(None);
    }

    // Try to parse as a list with arbitrary expression: #[state(default = expr)]
    // We need custom parsing since syn's Meta::NameValue only accepts literals
    let parser = |input: syn::parse::ParseStream| -> syn::Result<Option<syn::Expr>> {
        let content;
        syn::parenthesized!(content in input);

        // Look for "default = expr"
        let ident: syn::Ident = content.parse()?;
        if ident != "default" {
            return Err(syn::Error::new(ident.span(), "expected 'default'"));
        }
        content.parse::<syn::Token![=]>()?;
        let expr: syn::Expr = content.parse()?;
        Ok(Some(expr))
    };

    match parser.parse2(attr.tokens.clone()) {
        Ok(expr) => Ok(expr),
        Err(_) => {
            // Fall back to Meta parsing for simple literals
            let meta = attr.parse_meta()?;
            match meta {
                Meta::Path(_) => Ok(None),
                Meta::List(list) => {
                    for nested in &list.nested {
                        if let NestedMeta::Meta(Meta::NameValue(nv)) = nested {
                            if nv.path.is_ident("default") {
                                let expr = lit_to_expr(&nv.lit)?;
                                return Ok(Some(expr));
                            }
                        }
                    }
                    Ok(None)
                }
                Meta::NameValue(_) => {
                    Err(syn::Error::new_spanned(attr, "expected #[state] or #[state(default = expr)]"))
                }
            }
        }
    }
}

/// Convert a syn::Lit to a syn::Expr
fn lit_to_expr(lit: &Lit) -> Result<syn::Expr, syn::Error> {
    Ok(syn::Expr::Lit(syn::ExprLit {
        attrs: vec![],
        lit: lit.clone(),
    }))
}

/// Classify all fields in a struct as either config or state
fn classify_fields(fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>) -> Result<Vec<ClassifiedField>, syn::Error> {
    let mut classified = Vec::new();

    for field in fields {
        let mut has_config = false;
        let mut has_state = false;
        let mut config_default = None;
        let mut state_default = None;

        for attr in &field.attrs {
            if attr.path.is_ident("config") {
                if has_state {
                    return Err(syn::Error::new_spanned(
                        field,
                        format!("field '{}' cannot have both #[config] and #[state] attributes",
                            field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default())
                    ));
                }
                has_config = true;
                config_default = parse_config_attr(attr)?;
            } else if attr.path.is_ident("state") {
                if has_config {
                    return Err(syn::Error::new_spanned(
                        field,
                        format!("field '{}' cannot have both #[config] and #[state] attributes",
                            field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default())
                    ));
                }
                has_state = true;
                state_default = parse_state_attr(attr)?;
            }
        }

        // Require explicit annotation
        if !has_config && !has_state {
            return Err(syn::Error::new_spanned(
                field,
                format!("field '{}' must have either #[config] or #[state] attribute",
                    field.ident.as_ref().map(|i| i.to_string()).unwrap_or_default())
            ));
        }

        let kind = if has_config {
            FieldKind::Config { default_expr: config_default }
        } else {
            FieldKind::State { default_expr: state_default }
        };

        // Create a new field without the config/state attributes
        let mut clean_field = field.clone();
        clean_field.attrs.retain(|attr| {
            !attr.path.is_ident("config") && !attr.path.is_ident("state")
        });

        classified.push(ClassifiedField {
            field: clean_field,
            kind,
        });
    }

    Ok(classified)
}

/// Generate the config struct from classified fields
fn generate_config_struct(
    struct_name: &syn::Ident,
    vis: &syn::Visibility,
    fields: &[ClassifiedField],
    struct_attrs: &[&syn::Attribute],
) -> proc_macro2::TokenStream {
    let config_name = syn::Ident::new(&format!("{}Config", struct_name), struct_name.span());

    // Collect config fields only
    let config_fields: Vec<_> = fields.iter()
        .filter(|f| matches!(f.kind, FieldKind::Config { .. }))
        .map(|f| &f.field)
        .collect();

    // Generate Default impl with custom defaults
    let default_field_inits: Vec<_> = fields.iter()
        .filter_map(|f| {
            if let FieldKind::Config { default_expr } = &f.kind {
                let field_name = &f.field.ident;
                let init = match default_expr {
                    Some(expr) => quote! { #field_name: #expr },
                    None => quote! { #field_name: Default::default() },
                };
                Some(init)
            } else {
                None
            }
        })
        .collect();

    quote! {
        #(#struct_attrs)*
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(default, rename_all = "camelCase")]
        #vis struct #config_name {
            #(#config_fields),*
        }

        impl Default for #config_name {
            fn default() -> Self {
                Self {
                    #(#default_field_inits),*
                }
            }
        }
    }
}

/// Generate the rewritten node struct with config field and state fields
fn generate_node_struct(
    struct_name: &syn::Ident,
    vis: &syn::Visibility,
    fields: &[ClassifiedField],
    struct_attrs: &[&syn::Attribute],
) -> proc_macro2::TokenStream {
    let config_name = syn::Ident::new(&format!("{}Config", struct_name), struct_name.span());

    // Collect state fields only
    let state_fields: Vec<_> = fields.iter()
        .filter(|f| matches!(f.kind, FieldKind::State { .. }))
        .map(|f| &f.field)
        .collect();

    // Generate state field initializers for new()
    let state_field_inits: Vec<_> = fields.iter()
        .filter_map(|f| {
            if let FieldKind::State { default_expr } = &f.kind {
                let field_name = &f.field.ident;
                let init = match default_expr {
                    Some(expr) => quote! { #field_name: #expr },
                    None => quote! { #field_name: Default::default() },
                };
                Some(init)
            } else {
                None
            }
        })
        .collect();

    quote! {
        #(#struct_attrs)*
        #vis struct #struct_name {
            /// Node configuration
            pub config: #config_name,
            #(#state_fields),*
        }

        impl #struct_name {
            /// Create a new node with the given configuration
            pub fn new(config: #config_name) -> Self {
                Self {
                    config,
                    #(#state_field_inits),*
                }
            }

            /// Create a new node with default configuration
            pub fn with_default() -> Self {
                Self::new(#config_name::default())
            }
        }
    }
}

/// Generate the NodeConfigSchema implementation for the config struct
fn generate_schema_impl(
    struct_name: &syn::Ident,
    args: &NodeArgs,
) -> proc_macro2::TokenStream {
    let config_name = syn::Ident::new(&format!("{}Config", struct_name), struct_name.span());

    // Default node_type: remove "Node" suffix from struct name
    let node_type = args.node_type.clone().unwrap_or_else(|| {
        let name = struct_name.to_string();
        name.strip_suffix("Node")
            .unwrap_or(&name)
            .to_string()
    });

    let category = args.category.as_ref()
        .map(|c| quote! { Some(#c.to_string()) })
        .unwrap_or(quote! { None });

    let description = args.description.as_ref()
        .map(|d| quote! { Some(#d.to_string()) })
        .unwrap_or(quote! { None });

    let accepts_tokens = parse_runtime_data_types(&args.accepts);
    let produces_tokens = parse_runtime_data_types(&args.produces);
    let multi_output = args.multi_output;

    quote! {
        impl remotemedia_runtime_core::nodes::schema::NodeConfigSchema for #config_name {
            fn node_type() -> &'static str {
                #node_type
            }

            fn category() -> Option<String> {
                #category
            }

            fn description() -> Option<String> {
                #description
            }

            fn accepts() -> Vec<remotemedia_runtime_core::nodes::schema::RuntimeDataType> {
                vec![#accepts_tokens]
            }

            fn produces() -> Vec<remotemedia_runtime_core::nodes::schema::RuntimeDataType> {
                vec![#produces_tokens]
            }

            fn multi_output() -> bool {
                #multi_output
            }

            fn config_json_schema() -> serde_json::Value {
                let schema = schemars::schema_for!(#config_name);
                serde_json::to_value(schema).unwrap_or_default()
            }

            fn default_config() -> Option<serde_json::Value>
            where
                Self: Default + serde::Serialize,
            {
                serde_json::to_value(Self::default()).ok()
            }
        }
    }
}

/// Generate the AsyncStreamingNode trait implementation
fn generate_trait_impl(
    struct_name: &syn::Ident,
    args: &NodeArgs,
) -> proc_macro2::TokenStream {
    // Default node_type: remove "Node" suffix from struct name
    let node_type = args.node_type.clone().unwrap_or_else(|| {
        let name = struct_name.to_string();
        name.strip_suffix("Node")
            .unwrap_or(&name)
            .to_string()
    });

    let multi_output = args.multi_output;

    // For multi_output nodes, process() returns an error, process_streaming is used
    let process_impl = if multi_output {
        quote! {
            async fn process(&self, _data: remotemedia_runtime_core::data::RuntimeData) -> Result<remotemedia_runtime_core::data::RuntimeData, remotemedia_runtime_core::Error> {
                Err(remotemedia_runtime_core::Error::Execution(
                    concat!(stringify!(#struct_name), " requires streaming mode - use process_streaming()").into()
                ))
            }
        }
    } else {
        quote! {
            async fn process(&self, data: remotemedia_runtime_core::data::RuntimeData) -> Result<remotemedia_runtime_core::data::RuntimeData, remotemedia_runtime_core::Error> {
                #struct_name::process_impl(self, data).await
            }
        }
    };

    // For multi_output nodes, also generate process_streaming
    let streaming_impl = if multi_output {
        quote! {
            async fn process_streaming<F>(
                &self,
                data: remotemedia_runtime_core::data::RuntimeData,
                session_id: Option<String>,
                callback: F,
            ) -> Result<usize, remotemedia_runtime_core::Error>
            where
                F: FnMut(remotemedia_runtime_core::data::RuntimeData) -> Result<(), remotemedia_runtime_core::Error> + Send,
            {
                #struct_name::process_streaming_impl(self, data, session_id, callback).await
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #[async_trait::async_trait]
        impl remotemedia_runtime_core::nodes::AsyncStreamingNode for #struct_name {
            fn node_type(&self) -> &str {
                #node_type
            }

            async fn initialize(&self) -> Result<(), remotemedia_runtime_core::Error> {
                Ok(())
            }

            #process_impl

            #streaming_impl
        }
    }
}

/// Generate inventory registration for the config struct
fn generate_inventory_registration(struct_name: &syn::Ident) -> proc_macro2::TokenStream {
    let config_name = syn::Ident::new(&format!("{}Config", struct_name), struct_name.span());

    quote! {
        inventory::submit! {
            remotemedia_runtime_core::nodes::schema::RegisteredNodeConfig::new(
                <#config_name as remotemedia_runtime_core::nodes::schema::NodeConfigSchema>::to_node_schema
            )
        }
    }
}

/// Unified node definition macro.
///
/// Combines config struct generation, `AsyncStreamingNode` trait implementation,
/// and TypeScript type registration into a single declarative definition.
///
/// # Field Attributes
///
/// - `#[config]` - Field included in generated `{NodeName}Config` struct
/// - `#[config(default = expr)]` - Config field with custom default value
/// - `#[state]` - Field kept in node struct but excluded from config
/// - `#[state(default = expr)]` - State field with custom default value
///
/// # Node Attributes
///
/// - `node_type` - Node type identifier (defaults to struct name minus "Node")
/// - `category` - Category for grouping ("audio", "video", "ml", "text", "utility")
/// - `description` - Human-readable description
/// - `accepts` - Accepted input types (comma-separated: "audio", "text", etc.)
/// - `produces` - Produced output types (comma-separated)
/// - `multi_output` - Flag for multi-output streaming nodes
///
/// # Example
///
/// ```ignore
/// #[node(
///     node_type = "Echo",
///     category = "utility",
///     accepts = "text",
///     produces = "text"
/// )]
/// pub struct EchoNode {
///     #[config(default = "Echo: ".to_string())]
///     pub prefix: String,
///
///     #[state]
///     call_count: u64,
/// }
///
/// impl EchoNode {
///     // For regular nodes, implement process_impl
///     async fn process_impl(&self, data: RuntimeData) -> Result<RuntimeData, Error> {
///         // Your implementation here
///     }
/// }
/// ```
#[proc_macro_attribute]
pub fn node(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let attr_args = parse_macro_input!(attr as syn::AttributeArgs);

    // Parse the attribute arguments
    let args = match parse_node_args(&attr_args) {
        Ok(args) => args,
        Err(e) => return e.into_compile_error().into(),
    };

    let struct_name = &input.ident;
    let vis = &input.vis;

    // Extract fields from struct
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) => &named.named,
            _ => return syn::Error::new_spanned(&input, "#[node] only supports structs with named fields")
                .into_compile_error().into(),
        },
        _ => return syn::Error::new_spanned(&input, "#[node] only supports structs")
            .into_compile_error().into(),
    };

    // Classify fields as config or state
    let classified_fields = match classify_fields(fields) {
        Ok(fields) => fields,
        Err(e) => return e.into_compile_error().into(),
    };

    // Preserve doc comments and other attributes from original struct
    let struct_attrs: Vec<_> = input.attrs.iter()
        .filter(|attr| attr.path.is_ident("doc") || attr.path.is_ident("allow"))
        .collect();

    // Generate all components
    let config_struct = generate_config_struct(struct_name, vis, &classified_fields, &struct_attrs);
    let node_struct = generate_node_struct(struct_name, vis, &classified_fields, &struct_attrs);
    let schema_impl = generate_schema_impl(struct_name, &args);
    let trait_impl = generate_trait_impl(struct_name, &args);
    let inventory_reg = generate_inventory_registration(struct_name);

    let expanded = quote! {
        #config_struct
        #node_struct
        #schema_impl
        #trait_impl
        #inventory_reg
    };

    TokenStream::from(expanded)
}

// =============================================================================
// #[node_config] macro - Original config-only macro (preserved for compatibility)
// =============================================================================

/// Attribute macro for node configuration structs.
///
/// Automatically derives all necessary traits, adds serde attributes,
/// and registers the schema for TypeScript type generation.
///
/// # Attributes
///
/// - `node_type`: The node type name (defaults to struct name without "Config" suffix)
/// - `category`: Category for grouping ("audio", "video", "ml", "text", "utility")
/// - `description`: Human-readable description
/// - `accepts`: Accepted input types (comma-separated)
/// - `produces`: Produced output types (comma-separated)
/// - `multi_output`: Whether this node produces multiple outputs per input
///
/// # Example
///
/// ```ignore
/// #[node_config(node_type = "AudioResample", category = "audio")]
/// pub struct AudioResampleConfig {
///     pub target_sample_rate: u32,
/// }
/// ```
#[proc_macro_attribute]
pub fn node_config(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    let attr_args = parse_macro_input!(attr as syn::AttributeArgs);

    // Parse the attribute arguments
    let args = match parse_node_config_args(&attr_args) {
        Ok(args) => args,
        Err(e) => return e.into_compile_error().into(),
    };

    let struct_name = &input.ident;
    let vis = &input.vis;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    // Extract fields from struct
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(fields) => &fields.named,
            _ => return syn::Error::new_spanned(&input, "node_config only supports structs with named fields")
                .into_compile_error().into(),
        },
        _ => return syn::Error::new_spanned(&input, "node_config only supports structs")
            .into_compile_error().into(),
    };

    // Preserve doc comments and other attributes from original struct
    let struct_attrs: Vec<_> = input.attrs.iter()
        .filter(|attr| attr.path.is_ident("doc") || attr.path.is_ident("allow"))
        .collect();

    // Default node_type: remove "Config" suffix from struct name
    let node_type = args.node_type.unwrap_or_else(|| {
        let name = struct_name.to_string();
        name.strip_suffix("Config")
            .unwrap_or(&name)
            .to_string()
    });

    let category = args.category.map(|c| quote! { Some(#c.to_string()) }).unwrap_or(quote! { None });
    let description = args.description.map(|d| quote! { Some(#d.to_string()) }).unwrap_or(quote! { None });

    // Parse accepts/produces as RuntimeDataType variants
    let accepts_tokens = parse_runtime_data_types(&args.accepts);
    let produces_tokens = parse_runtime_data_types(&args.produces);

    let multi_output = args.multi_output;
    let generics = &input.generics;

    let expanded = quote! {
        #(#struct_attrs)*
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(default, rename_all = "camelCase")]
        #vis struct #struct_name #generics #where_clause {
            #fields
        }

        // Implement the NodeConfigSchema trait
        impl #impl_generics crate::nodes::schema::NodeConfigSchema for #struct_name #ty_generics #where_clause {
            fn node_type() -> &'static str {
                #node_type
            }

            fn category() -> Option<String> {
                #category
            }

            fn description() -> Option<String> {
                #description
            }

            fn accepts() -> Vec<crate::nodes::schema::RuntimeDataType> {
                vec![#accepts_tokens]
            }

            fn produces() -> Vec<crate::nodes::schema::RuntimeDataType> {
                vec![#produces_tokens]
            }

            fn multi_output() -> bool {
                #multi_output
            }

            fn config_json_schema() -> serde_json::Value {
                let schema = schemars::schema_for!(#struct_name);
                serde_json::to_value(schema).unwrap_or_default()
            }

            fn default_config() -> Option<serde_json::Value>
            where
                Self: Default + serde::Serialize,
            {
                serde_json::to_value(Self::default()).ok()
            }
        }

        // Auto-register via inventory
        inventory::submit! {
            crate::nodes::schema::RegisteredNodeConfig::new(
                <#struct_name as crate::nodes::schema::NodeConfigSchema>::to_node_schema
            )
        }
    };

    TokenStream::from(expanded)
}

/// Parsed arguments from `#[node_config(...)]`
struct NodeConfigArgs {
    node_type: Option<String>,
    category: Option<String>,
    description: Option<String>,
    accepts: Option<String>,
    produces: Option<String>,
    multi_output: bool,
}

fn parse_node_config_args(args: &[NestedMeta]) -> Result<NodeConfigArgs, syn::Error> {
    let mut node_type = None;
    let mut category = None;
    let mut description = None;
    let mut accepts = None;
    let mut produces = None;
    let mut multi_output = false;

    for arg in args {
        match arg {
            NestedMeta::Meta(Meta::NameValue(nv)) => {
                let ident = nv.path.get_ident()
                    .ok_or_else(|| syn::Error::new_spanned(&nv.path, "expected identifier"))?;

                match ident.to_string().as_str() {
                    "node_type" => {
                        if let Lit::Str(s) = &nv.lit {
                            node_type = Some(s.value());
                        }
                    }
                    "category" => {
                        if let Lit::Str(s) = &nv.lit {
                            category = Some(s.value());
                        }
                    }
                    "description" => {
                        if let Lit::Str(s) = &nv.lit {
                            description = Some(s.value());
                        }
                    }
                    "accepts" => {
                        if let Lit::Str(s) = &nv.lit {
                            accepts = Some(s.value());
                        }
                    }
                    "produces" => {
                        if let Lit::Str(s) = &nv.lit {
                            produces = Some(s.value());
                        }
                    }
                    "multi_output" => {
                        if let Lit::Bool(b) = &nv.lit {
                            multi_output = b.value;
                        }
                    }
                    other => {
                        return Err(syn::Error::new_spanned(&nv.path, format!("unknown attribute: {}", other)));
                    }
                }
            }
            NestedMeta::Meta(Meta::Path(path)) => {
                if path.is_ident("multi_output") {
                    multi_output = true;
                }
            }
            _ => {}
        }
    }

    Ok(NodeConfigArgs {
        node_type,
        category,
        description,
        accepts,
        produces,
        multi_output,
    })
}

/// Parse comma-separated RuntimeDataType names into tokens
fn parse_runtime_data_types(types: &Option<String>) -> proc_macro2::TokenStream {
    match types {
        Some(s) if !s.is_empty() => {
            let types: Vec<_> = s
                .split(',')
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .map(|t| {
                    let variant = match t.to_lowercase().as_str() {
                        "audio" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Audio },
                        "video" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Video },
                        "json" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Json },
                        "text" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Text },
                        "binary" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Binary },
                        "tensor" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Tensor },
                        "numpy" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::Numpy },
                        "control" | "controlmessage" => quote! { remotemedia_runtime_core::nodes::schema::RuntimeDataType::ControlMessage },
                        _ => quote! { compile_error!(concat!("Unknown RuntimeDataType: ", #t)) },
                    };
                    variant
                })
                .collect();
            quote! { #(#types),* }
        }
        _ => quote! {},
    }
}
