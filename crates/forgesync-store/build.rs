//! Rebuilds the crate when migrations change, since SQLx embeds them at compile time.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
