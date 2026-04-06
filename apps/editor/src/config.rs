//! CLI and environment configuration (embedded mode, etc.).

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
    if std::env::args()
        .skip(1)
        .any(|a| a == "--embedded" || a == "-e")
    {
        return true;
    }
    std::env::var("VGE_EMBEDDED")
        .map(|s| {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}
