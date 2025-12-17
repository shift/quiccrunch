fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Skip proto compilation for now - use manual implementation
    println!("cargo:rerun-if-changed=../../proto/benchmark.proto");
    Ok(())
}