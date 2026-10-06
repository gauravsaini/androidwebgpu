use std::env;
use std::path::PathBuf;
use std::process::Command;

fn git(root: &PathBuf, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git {:?}: {error}", args));
    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .expect("git returned non-UTF8 output")
        .trim()
        .to_owned()
}

fn watch_git_path(root: &PathBuf, path: &str) {
    let absolute = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", path],
    );
    println!("cargo:rerun-if-changed={absolute}");
}

fn main() {
    println!("cargo:rerun-if-env-changed=PATHN_BUILD_GIT_REV");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = PathBuf::from(git(&manifest_dir, &["rev-parse", "--show-toplevel"]));
    let revision = git(&root, &["rev-parse", "HEAD"]);
    if let Ok(requested) = env::var("PATHN_BUILD_GIT_REV") {
        if requested != revision {
            panic!("PATHN_BUILD_GIT_REV={requested} does not match checked-out HEAD {revision}");
        }
    }

    println!("cargo:rustc-env=BINARY_GIT_REV={revision}");
    watch_git_path(&root, "HEAD");
    watch_git_path(&root, "packed-refs");
    if let Ok(reference) = Command::new("git")
        .args(["symbolic-ref", "-q", "HEAD"])
        .current_dir(&root)
        .output()
    {
        if reference.status.success() {
            let name = String::from_utf8_lossy(&reference.stdout).trim().to_owned();
            watch_git_path(&root, &name);
        }
    }
}
