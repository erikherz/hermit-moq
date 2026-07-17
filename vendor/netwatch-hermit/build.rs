#![allow(semicolon_in_expressions_from_macros)]
use cfg_aliases::cfg_aliases;

fn main() {
    // Setup cfg aliases
    cfg_aliases! {
        // Convenience aliases
        wasm_browser: { all(target_family = "wasm", target_os = "unknown") },
        // Limited POSIX platforms (not wasm)
        posix_minimal: { any(target_os = "espidf", target_os = "hermit") },
    }
}
