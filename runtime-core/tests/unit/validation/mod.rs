//! Unit tests for node parameter validation
//!
//! Tests organized by User Story per tasks.md

mod test_type_errors;
mod test_required;
mod test_range;
mod test_enum;
mod test_nested;
mod test_error_messages;

// User Story 4: Node developers define validation rules
mod test_macro_schema;
mod test_schemars_integration;

// User Story 5: Operators can introspect valid node parameters
mod test_introspection;

// Phase 8: Edge cases and backward/forward compatibility
mod test_backward_compat;
mod test_forward_compat;
mod test_defaults;
