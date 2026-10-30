fn main() {
    use std::process::Command;
    let git = |args: &[&str]| -> String {
        let result = Command::new("git")
            .args(args)
            .output()
            .expect("Git is required for build provenance");
        assert!(result.status.success(), "Cannot read source provenance");
        String::from_utf8(result.stdout).unwrap().trim().to_string()
    };
    let commit = git(&["rev-parse", "HEAD"]);
    assert!(commit.len() == 40 && commit.bytes().all(|c| c.is_ascii_hexdigit()));
    let dirty = !git(&["status", "--porcelain", "--untracked-files=normal"]).is_empty();
    for reference in [
        "HEAD".to_string(),
        git(&["rev-parse", "--symbolic-full-name", "HEAD"]),
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            git(&["rev-parse", "--git-path", &reference])
        );
    }
    for path in ["src", "ui", "Cargo.toml", "Cargo.lock", "build.rs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rustc-env=ORDIS_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=ORDIS_BUILD_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=ORDIS_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    tauri_build::build()
}
