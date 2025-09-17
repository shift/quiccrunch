Warning: No write access to CPU frequency controls.
Run benchmarks with sudo for full frequency scaling support.
QuicCrunch development environment loaded!
- Rust toolchain: rustc 1.89.0 (29483883e 2025-08-04)
- Python: Python 3.13.5

To build Python bindings, run: cd bindings/python && maturin develop
#[tokio::main]
async fn main() {
    println!("Hello from tokio!");
}
