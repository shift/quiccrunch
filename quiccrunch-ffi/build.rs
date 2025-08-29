use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    
    // Generate C header file
    if cfg!(feature = "generate-headers") {
        let config = cbindgen::Config::from_file("cbindgen.toml")
            .expect("Unable to find cbindgen.toml configuration file");
        
        cbindgen::generate_with_config(&crate_dir, config)
            .expect("Unable to generate bindings")
            .write_to_file("quiccrunch.h");
        
        println!("cargo:warning=Generated quiccrunch.h header file");
        println!("cargo:rerun-if-changed=cbindgen.toml");
    }
    
    println!("cargo:rerun-if-changed=src/");
}