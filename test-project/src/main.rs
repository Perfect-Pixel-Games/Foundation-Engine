#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

//! Thin binary wrapper for the Foundation engine test fixture.
//!
//! Development should normally launch through Foundation:
//! `cargo run -p foundation -- --project test-project`.

fn main() -> bevy::prelude::AppExit {
    test_project::run()
}
