// compile-flags: -Z control-flow-guard=no

#![crate_type = "lib"]

// A basic test function.
pub fn test() {
}

// Ensure the module flag cfguard is not present
// CHECK-NOT: !"cfguard"
