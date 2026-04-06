//! CLI and environment configuration (embedded mode, etc.).

use tracing::debug;

/// True if the user asked for in-process Vulkan + egui.
///
/// - **CLI:** pass `--embedded` or `-e` (works on all shells).
/// - **Env:** `VGE_EMBEDDED` set to `1`, `true`, `yes`, or `on` (case-insensitive).
///
/// Note: POSIX syntax `VGE_EMBEDDED=1 cargo run …` does **not** set the variable on **cmd.exe**
/// or **PowerShell**. Use:
///
/// - `cargo run -p editor -- --embedded`
/// - PowerShell: `$env:VGE_EMBEDDED = "1"; cargo run -p editor`
/// - cmd: `set VGE_EMBEDDED=1 && cargo run -p editor`
pub fn embedded_mode_requested() -> bool {
    let from_cli = std::env::args()
        .skip(1)
        .any(|a| a == "--embedded" || a == "-e");
    if from_cli {
        debug!(target: "vge_embedded", "embedded_mode_requested: true (--embedded or -e)");
        return true;
    }
    let from_env = std::env::var("VGE_EMBEDDED")
        .map(|s| {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    debug!(
        target: "vge_embedded",
        vge_embedded = ?std::env::var("VGE_EMBEDDED").ok(),
        from_env,
        "embedded_mode_requested (env)"
    );
    from_env
}
