//! # Declare migration inputs for Cargo
//!
//! This build script emits Cargo rerun directives for the `migrations` directory and each entry
//! it can enumerate. SQLx embeds migrations when the store crate is compiled; declaring their
//! source files prevents schema edits from leaving an already compiled migration set unchanged.
//! The directory directive also lets Cargo notice changes to the set of migration files.
//!
//! Paths are relative to the store package's build-script working directory. This script does not
//! parse SQL, validate migration ordering, generate a database, or connect to an application
//! archive. Compilation and SQLx embedding own schema inputs; runtime archive lifecycle remains
//! in the archive and migration modules, where callers choose creation or migration explicitly.
//!
//! Enumeration is best-effort: the directory directive is always emitted, while an unreadable
//! directory or individual entry contributes no per-entry directive. This script does not convert
//! those filesystem failures into a custom build error or certify that migrations are usable.

use std::path::Path;

/// Emits migration-file dependencies so Cargo regenerates embedded SQL after schema edits.
fn main() {
    let migrations = Path::new("migrations");
    println!("cargo:rerun-if-changed={}", migrations.display());
    if let Ok(entries) = std::fs::read_dir(migrations) {
        for entry in entries.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
}
