// Records the commit the binary was built from, so problem reports can name it. Without git (a source tarball)
// the commit is simply left out.
fn main() {
    let git = |args: &[&str]| {
        std::process::Command::new("git").args(args).output().ok().filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    if let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(commit) = git(&["rev-parse", "--short=10", "HEAD"]) {
        println!("cargo:rustc-env=OUNCE_GIT_COMMIT={commit}");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
