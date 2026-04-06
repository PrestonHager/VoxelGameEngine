//! Window helpers and `winit` re-exports for the engine.

pub use raw_window_handle;
pub use winit;

use winit::dpi::LogicalSize;
use winit::window::WindowAttributes;

pub fn default_window_attributes() -> WindowAttributes {
    WindowAttributes::default()
        .with_title("Voxel Engine")
        .with_inner_size(LogicalSize::new(1280.0, 720.0))
}
