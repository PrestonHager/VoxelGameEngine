use naga::back::spv;
use naga::valid::{Capabilities, ValidationFlags, Validator};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let shader_dir = manifest_dir.join("shaders");
    println!("cargo:rerun-if-changed={}", shader_dir.display());

    let wgsl_path = shader_dir.join("mesh.wgsl");
    let wgsl = fs::read_to_string(&wgsl_path).expect("read mesh.wgsl");
    let mut front = naga::front::wgsl::Frontend::new();
    let module = front.parse(&wgsl).expect("parse wgsl");

    let caps = Capabilities::all();
    let mut validator = Validator::new(ValidationFlags::all(), caps);
    let module_info = validator.validate(&module).expect("validate naga module");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let opts = spv::Options::default();
    let spv_words = spv::write_vec(&module, &module_info, &opts, None).expect("spv write");

    let mut spv_bytes = Vec::with_capacity(spv_words.len() * 4);
    for word in &spv_words {
        spv_bytes.extend_from_slice(&word.to_le_bytes());
    }
    fs::write(out_dir.join("mesh.spv"), spv_bytes).expect("write mesh.spv");
}
