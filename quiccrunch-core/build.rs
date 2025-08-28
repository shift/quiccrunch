use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=schema/frame.capnp");
    println!("cargo:rerun-if-changed=schema/rpc.capnp");
    println!("cargo:rerun-if-changed=proto/benchmark.proto");
    println!("cargo:rerun-if-changed=build.rs");
    
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let schema_out = out_dir.join("schema");
    
    // Create schema output directory
    std::fs::create_dir_all(&schema_out).unwrap();
    
    // Compile Cap'n Proto schemas
    capnpc::CompilerCommand::new()
        .src_prefix("schema")
        .file("schema/frame.capnp")
        .file("schema/rpc.capnp")
        .output_path(&schema_out)
        .run()
        .expect("Failed to compile Cap'n Proto schemas");
        
    // Fix up module paths in generated code
    for schema_name in &["frame_capnp.rs", "rpc_capnp.rs"] {
        let generated_file = schema_out.join(schema_name);
        if generated_file.exists() {
            let content = std::fs::read_to_string(&generated_file).unwrap();
            
            // Replace crate references to use schema module prefix
            let fixed_content = content
                .replace("crate::frame_capnp", "crate::schema::frame_capnp")
                .replace("crate::rpc_capnp", "crate::schema::rpc_capnp");
            
            std::fs::write(generated_file, fixed_content).unwrap();
        }
    }
    
    // Compile Protocol Buffers for gRPC comparison
    if std::path::Path::new("proto/benchmark.proto").exists() {
        tonic_build::configure()
            .build_server(true)
            .build_client(true)
            .out_dir(&out_dir)
            .compile(&["proto/benchmark.proto"], &["proto"])
            .expect("Failed to compile protobuf schema");
    }
}