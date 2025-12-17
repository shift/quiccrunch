fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    // Add any build-time configuration here
    // Cap'n Proto compilation will be added later for binary serialization optimization
}