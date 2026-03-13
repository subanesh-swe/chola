use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);

    let workspace_root = manifest_dir
        .ancestors()
        .find(|p| p.join("proto").exists())
        .ok_or("Could not find workspace root containing 'proto' directory")?;

    let proto_dir = workspace_root.join("proto");
    let proto_file = proto_dir.join("orchestrator.proto");

    // Tell Cargo to RE-RUN this script if the proto file changes
    // Without this, changes to orchestrator.proto won't trigger a rebuild!
    println!("cargo:rerun-if-changed={}", proto_file.display());

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[proto_file], &[proto_dir])?;

    Ok(())
}
