use std::process::Command;
use std::{fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=themes");
    embed_themes();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let version = Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_string())
        .unwrap_or_else(|| "rustc (unknown version)".into());
    println!("cargo:rustc-env=BTOPRS_RUSTC_VERSION={version}");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());
    println!(
        "cargo:rustc-env=BTOPRS_BUILD_CONFIGURATION=profile={profile} target={target} no-external-crates"
    );
}

fn embed_themes() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let mut paths: Vec<_> = fs::read_dir(root.join("themes"))
        .expect("read bundled themes")
        .map(|entry| entry.expect("read theme entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "theme")
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no bundled themes found");
    let mut source = String::from("const BUNDLED_THEMES: &[(&str, &str)] = &[\n");
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("UTF-8 theme name");
        let contents = fs::read_to_string(&path).expect("read theme contents");
        source.push_str(&format!("({name:?}, {contents:?}),\n"));
    }
    source.push_str("];\n");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("build output directory"));
    fs::write(output.join("bundled_themes.rs"), source).expect("write embedded themes");
}
