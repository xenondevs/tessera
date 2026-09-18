use std::env;
use std::path::PathBuf;

fn main() {
    let libs = toml::from_str::<toml::Value>(include_str!("../../gradle/libs.versions.toml")).unwrap();
    let minecraft_version = libs.get("versions").and_then(|v| v.get("minecraft")).and_then(|v| v.as_str()).unwrap();

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let input = manifest.join("capture");

    println!("cargo:rerun-if-changed={}", input.display());
    println!("cargo:rustc-env=TESSERA_CAPTURE_MINECRAFT={minecraft_version}");

    match tessera_capture_gen::generate::generate(&input, minecraft_version, &out) {
        Ok(stales) => {
            for stale in stales {
                println!("cargo:warning=ignored discard matched nothing: {stale}");
            }
        }
        Err(err) => panic!("capture gen failed: {err}"),
    }
}
