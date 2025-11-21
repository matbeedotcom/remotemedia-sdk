//! Registration macros for ergonomic node registration
//!
//! This module provides declarative macros that simplify node registration
//! by automatically generating factory structs and handling Arc wrapping.
//!
//! # Macros
//!
//! - `register_python_node!` - Register a single Python node by class name
//! - `register_python_nodes!` - Register multiple Python nodes in batch
//! - `register_rust_node!` - Register a Rust node with custom initialization
//! - `register_rust_node_default!` - Register a Rust node using Default trait
//!
//! # Example
//!
//! ```ignore
//! use remotemedia_runtime_core::nodes::registry::NodeRegistry;
//! use remotemedia_runtime_core::register_python_node;
//!
//! let mut registry = NodeRegistry::new();
//! register_python_node!(registry, "OmniASRNode");
//! ```

/// Register a single Python node by class name
///
/// This macro simplifies Python node registration by automatically generating
/// the factory boilerplate. It reduces registration from ~40 lines to 1 line.
///
/// # Syntax
///
/// ```ignore
/// register_python_node!(registry, "NodeClassName");
/// ```
///
/// # Parameters
///
/// - `registry`: Mutable reference to `NodeRegistry`
/// - `node_name`: String literal - must match Python class name exactly
///
/// # Example
///
/// ```ignore
/// use remotemedia_runtime_core::nodes::registry::NodeRegistry;
/// use remotemedia_runtime_core::register_python_node;
///
/// let mut registry = NodeRegistry::new();
/// register_python_node!(registry, "OmniASRNode");
///
/// assert!(registry.has_python_impl("OmniASRNode"));
/// ```
///
/// # Generated Code
///
/// The macro generates an anonymous factory struct that implements `NodeFactory`:
///
/// ```ignore
/// struct Factory;
/// impl NodeFactory for Factory {
///     fn create(&self, _params: Value) -> Result<Box<dyn NodeExecutor>> {
///         Ok(Box::new(PythonNodeExecutor::new("OmniASRNode")))
///     }
///     fn node_type(&self) -> &str { "OmniASRNode" }
///     fn is_rust_native(&self) -> bool { false }
/// }
/// registry.register_python(Arc::new(Factory));
/// ```
#[macro_export]
macro_rules! register_python_node {
    ($registry:expr, $node_name:literal) => {{
        struct Factory;
        impl $crate::nodes::registry::NodeFactory for Factory {
            fn create(
                &self,
                _params: serde_json::Value,
            ) -> $crate::Result<Box<dyn $crate::executor::node_executor::NodeExecutor>>
            {
                Ok(Box::new(
                    $crate::executor::node_executor::PythonNodeExecutor::new($node_name),
                ))
            }

            fn node_type(&self) -> &str {
                $node_name
            }

            fn is_rust_native(&self) -> bool {
                false
            }
        }
        $registry.register_python(std::sync::Arc::new(Factory));
    }};
}

/// Register multiple Python nodes in a single batch operation
///
/// This macro simplifies batch registration of Python nodes by expanding to
/// multiple `register_python_node!` calls.
///
/// # Syntax
///
/// ```ignore
/// register_python_nodes!(registry, ["Node1", "Node2", "Node3"]);
/// ```
///
/// # Parameters
///
/// - `registry`: Mutable reference to `NodeRegistry`
/// - `node_names`: Array of string literals (supports trailing comma)
///
/// # Example
///
/// ```ignore
/// use remotemedia_runtime_core::nodes::registry::NodeRegistry;
/// use remotemedia_runtime_core::register_python_nodes;
///
/// let mut registry = NodeRegistry::new();
/// register_python_nodes!(registry, [
///     "OmniASRNode",
///     "KokoroTTSNode",
///     "SimplePyTorchNode",
/// ]);
///
/// assert_eq!(registry.list_node_types().len(), 3);
/// ```
///
/// # Behavior
///
/// Expands to multiple `register_python_node!` invocations:
///
/// ```ignore
/// register_python_nodes!(registry, ["Node1", "Node2"]);
///
/// // Expands to:
/// register_python_node!(registry, "Node1");
/// register_python_node!(registry, "Node2");
/// ```
///
/// Duplicate names are allowed (last registration wins).
#[macro_export]
macro_rules! register_python_nodes {
    ($registry:expr, [$($node_name:literal),* $(,)?]) => {{
        $(
            $crate::register_python_node!($registry, $node_name);
        )*
    }};
}

/// Register a Rust node with custom initialization closure
///
/// This macro simplifies Rust node registration by automatically generating
/// the factory boilerplate while allowing custom initialization logic.
///
/// # Syntax
///
/// ```ignore
/// register_rust_node!(registry, NodeType, |params| {
///     NodeType::new(params)
/// });
/// ```
///
/// # Parameters
///
/// - `registry`: Mutable reference to `NodeRegistry`
/// - `node_type`: Rust type implementing `NodeHandler` trait
/// - `factory`: Closure `Fn(Value) -> Result<NodeType>` - custom initialization logic
///
/// # Example
///
/// ```ignore
/// use remotemedia_runtime_core::nodes::registry::NodeRegistry;
/// use remotemedia_runtime_core::register_rust_node;
///
/// struct AudioChunkerNode {
///     chunk_size: usize,
/// }
///
/// impl NodeHandler for AudioChunkerNode { /* ... */ }
///
/// let mut registry = NodeRegistry::new();
/// register_rust_node!(registry, AudioChunkerNode, |params| {
///     let chunk_size = params.get("chunk_size")?.as_u64()? as usize;
///     Ok(AudioChunkerNode { chunk_size })
/// });
/// ```
///
/// # Guarantees
///
/// - Node name derived from type via `stringify!` (typo-proof)
/// - Closure invoked on each `create()` call (fresh instance)
/// - Closure can capture environment (must be `'static`)
/// - Factory errors propagate as `Result::Err`
#[macro_export]
macro_rules! register_rust_node {
    ($registry:expr, $node_type:ty, $factory:expr) => {{
        struct Factory;
        impl $crate::nodes::registry::NodeFactory for Factory {
            fn create(
                &self,
                params: serde_json::Value,
            ) -> $crate::Result<Box<dyn $crate::executor::node_executor::NodeExecutor>>
            {
                let handler: $node_type = ($factory)(params)?;
                Ok(Box::new(
                    $crate::executor::node_executor::RustNodeExecutor::new(
                        stringify!($node_type),
                        Box::new(handler),
                    ),
                ))
            }

            fn node_type(&self) -> &str {
                stringify!($node_type)
            }

            fn is_rust_native(&self) -> bool {
                true
            }
        }
        $registry.register_rust(std::sync::Arc::new(Factory));
    }};
}

/// Register a Rust node using Default trait
///
/// This macro is the simplest form for registering Rust nodes that implement
/// the `Default` trait. It delegates to `register_rust_node!` with a default closure.
///
/// # Syntax
///
/// ```ignore
/// register_rust_node_default!(registry, NodeType);
/// ```
///
/// # Parameters
///
/// - `registry`: Mutable reference to `NodeRegistry`
/// - `node_type`: Rust type implementing `NodeHandler + Default`
///
/// # Example
///
/// ```ignore
/// use remotemedia_runtime_core::nodes::registry::NodeRegistry;
/// use remotemedia_runtime_core::register_rust_node_default;
///
/// #[derive(Default)]
/// struct PassThroughNode;
///
/// impl NodeHandler for PassThroughNode { /* ... */ }
///
/// let mut registry = NodeRegistry::new();
/// register_rust_node_default!(registry, PassThroughNode);
/// ```
///
/// # Compile-time Requirements
///
/// The node type MUST implement the `Default` trait. If it doesn't, you'll get
/// a compile error. Use `register_rust_node!` instead for custom initialization.
#[macro_export]
macro_rules! register_rust_node_default {
    ($registry:expr, $node_type:ty) => {{
        $crate::register_rust_node!(
            $registry,
            $node_type,
            |_params: serde_json::Value| -> $crate::Result<$node_type> {
                Ok(<$node_type>::default())
            }
        );
    }};
}
