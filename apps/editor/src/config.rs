//! CLI and environment configuration (embedded mode, etc.).

use tracing::debug;

/// True when editor should run in-process Vulkan + egui.
///
/// Default is embedded mode.
///
/// - **CLI opt-out:** pass `--no-embedded` to force the external engine window mode.
/// - **CLI opt-in alias:** `--embedded` / `-e` (kept for compatibility).
/// - **Env override:** `VGE_EMBEDDED` set to `1/true/yes/on` (embedded) or
///   `0/false/no/off` (external), case-insensitive.
///
/// Note: POSIX syntax `VGE_EMBEDDED=0 cargo run …` does **not** set the variable on
/// **cmd.exe** or **PowerShell**. Use:
///
/// - `cargo run -p editor -- --no-embedded`
/// - PowerShell: `$env:VGE_EMBEDDED = "0"; cargo run -p editor`
/// - cmd: `set VGE_EMBEDDED=0 && cargo run -p editor`
pub fn embedded_mode_requested(default_embedded: bool) -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--no-embedded") {
        debug!(target: "vge_embedded", "embedded_mode_requested: false (--no-embedded)");
        return false;
    }
    if args.iter().any(|a| a == "--embedded" || a == "-e") {
        debug!(target: "vge_embedded", "embedded_mode_requested: true (--embedded or -e)");
        return true;
    }
    let from_env = std::env::var("VGE_EMBEDDED").ok().and_then(|s| {
        let v = s.trim().to_ascii_lowercase();
        if matches!(v.as_str(), "1" | "true" | "yes" | "on") {
            Some(true)
        } else if matches!(v.as_str(), "0" | "false" | "no" | "off") {
            Some(false)
        } else {
            None
        }
    });
    debug!(
        target: "vge_embedded",
        vge_embedded = ?std::env::var("VGE_EMBEDDED").ok(),
        from_env = ?from_env,
        "embedded_mode_requested (env)"
    );
    from_env.unwrap_or(default_embedded)
}
