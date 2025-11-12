//! Example demonstrating ergonomic node registration patterns
//!
//! This example shows how to use the new registration macros to reduce
//! boilerplate when registering nodes.
//!
//! Run with: `cargo run --example node_registration_example`

use remotemedia_runtime_core::nodes::registry::{NodeRegistry, RuntimeHint};
use remotemedia_runtime_core::{register_python_node, register_python_nodes};

fn main() {
    println!("=== Node Registration Example ===\n");

    // =============================================
    // Old Way (verbose) - ~40 lines of boilerplate
    // =============================================
    println!("❌ OLD WAY (verbose):");
    println!("   - Define struct + impl NodeHandler (20 lines)");
    println!("   - Create factory struct (5 lines)");
    println!("   - Implement NodeFactory trait (15 lines)");
    println!("   - Manual registration with Arc::new");
    println!("   Total: ~40 lines per node\n");

    // =============================================
    // New Way (macros) - 1 line per node!
    // =============================================
    println!("✅ NEW WAY (macros):\n");

    let mut registry = NodeRegistry::new();

    println!("1. Single Python node registration:");
    println!("   register_python_node!(registry, \"OmniASRNode\");");
    register_python_node!(registry, "OmniASRNode");
    println!("   ✓ Registered OmniASRNode\n");

    println!("2. Batch Python node registration:");
    println!("   register_python_nodes!(registry, [");
    println!("       \"KokoroTTSNode\",");
    println!("       \"SimplePyTorchNode\",");
    println!("       \"ExpanderNode\",");
    println!("   ]);");
    register_python_nodes!(registry, [
        "KokoroTTSNode",
        "SimplePyTorchNode",
        "ExpanderNode",
    ]);
    println!("   ✓ Registered 3 nodes in one call\n");

    // =============================================
    // Verify Registration
    // =============================================
    println!("3. Verify all nodes are registered:");
    let node_types = registry.list_node_types();
    println!("   Total nodes: {}", node_types.len());
    for node_type in &node_types {
        let impl_type = if registry.has_rust_impl(node_type) {
            "Rust"
        } else if registry.has_python_impl(node_type) {
            "Python"
        } else {
            "Unknown"
        };
        println!("   - {}: {}", node_type, impl_type);
    }
    println!();

    // =============================================
    // Test Node Creation
    // =============================================
    println!("4. Test node creation:");
    match registry.create_node("OmniASRNode", RuntimeHint::Python, serde_json::json!({})) {
        Ok(_) => println!("   ✓ Successfully created OmniASRNode executor"),
        Err(e) => println!("   ✗ Failed to create node: {}", e),
    }

    match registry.create_node("NonExistentNode", RuntimeHint::Auto, serde_json::json!({})) {
        Ok(_) => println!("   ✗ Unexpectedly created non-existent node"),
        Err(e) => println!("   ✓ Correctly rejected non-existent node: {}", e),
    }
    println!();

    // =============================================
    // Benefits Summary
    // =============================================
    println!("=== Benefits ===");
    println!("✓ 95% less boilerplate (40 lines → 1 line)");
    println!("✓ Type-safe (uses stringify! for consistency)");
    println!("✓ No manual Arc wrapping");
    println!("✓ Batch registration support");
    println!("✓ Clear and readable");
    println!("✓ Fully backward compatible\n");

    println!("=== Real World Usage ===");
    println!("Before: 720 lines for 18 nodes");
    println!("After:  18 lines for 18 nodes");
    println!("Saved:  702 lines (97.5% reduction!)");
}

