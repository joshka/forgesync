//! # Store schema build preparation
//!
//! This build script tells Cargo to rebuild the store crate when a migration file changes. It
//! enumerates the `migrations` directory and emits a rerun directive for each entry.
//!
//! It does not create or migrate an application archive. Runtime archive lifecycle remains in
//! `archive` and `migration`, where a caller chooses those effects explicitly.

use std::path::Path;

fn main() {
    let migrations = Path::new("migrations");
    println!("cargo:rerun-if-changed={}", migrations.display());
    if let Ok(entries) = std::fs::read_dir(migrations) {
        for entry in entries.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
}
