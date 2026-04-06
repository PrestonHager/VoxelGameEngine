//! In-process editor + Vulkan using winit child window (embedded view).
//!
//! Set `VGE_EMBEDDED=1` to use this path instead of eframe + external `engine-runner`.
//!
//! # Safety contract
//!
//! The embedded path uses a **single-threaded** winit event loop. `EditorModel` and
//! `EngineState` live on `EmbeddedApp` and are not shared across threads. The raw
//! pointers passed into `egui_glow::run` are only dereferenced for the synchronous
//! duration of that closure (same stack frame as the event handler); there is no
//! re-entrancy from other threads into that closure.
//!
//! Glutin, OpenGL (`glow`), and Vulkan require `unsafe` per their APIs; each site below
//! includes a `// SAFETY:` note.

use crate::model::EditorModel;
use crate::ui::draw_editor_ui;
use egui_winit::winit;
use engine_core::EngineState;
use render_vulkan::{RenderError, VulkanRenderer};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::CursorGrabMode;
use winit::window::{Window, WindowAttributes, WindowId};

#[derive(Debug)]
enum UserEvent {
    Redraw(std::time::Duration),
}

/// Glutin + OpenGL context for egui (adapted from `egui_glow` `pure_glow` example).
struct GlutinWindowContext {
    window: winit::window::Window,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_display: glutin::display::Display,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
}

impl GlutinWindowContext {
    /// Create GL display, context, and surface for the editor window.
    ///
    /// # Safety
    ///
    /// Must be called on the thread that owns `event_loop`, with valid GL config and
    /// window handles as required by glutin.
    unsafe fn new(event_loop: &ActiveEventLoop) -> Self {
        use glutin::context::NotCurrentGlContext;
        use glutin::display::GetGlDisplay;
        use glutin::display::GlDisplay;
        use glutin::prelude::GlSurface;

        #[cfg(target_os = "windows")]
        let mut winit_window_builder = winit::window::WindowAttributes::default()
            .with_resizable(true)
            .with_inner_size(LogicalSize {
                width: 1200.0,
                height: 780.0,
            })
            .with_title("Voxel Editor")
            .with_visible(false);
        #[cfg(not(target_os = "windows"))]
        let winit_window_builder = winit::window::WindowAttributes::default()
            .with_resizable(true)
            .with_inner_size(LogicalSize {
                width: 1200.0,
                height: 780.0,
            })
            .with_title("Voxel Editor")
            .with_visible(false);

        // WS_CLIPCHILDREN: parent GDI/OpenGL must not paint over embedded child HWND regions
        // (reduces "ghost" / smear artifacts next to the Vulkan view on Windows).
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            winit_window_builder = winit_window_builder.with_clip_children(true);
        }

        let config_template_builder = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        let (mut window, gl_config) = glutin_winit::DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(winit_window_builder.clone()))
            .build(
                event_loop,
                config_template_builder,
                |mut config_iterator| {
                    config_iterator.next().expect(
                        "failed to find a matching configuration for creating glutin config",
                    )
                },
            )
            .expect("failed to create gl_config");

        let gl_display = gl_config.display();

        let raw_window_handle = window.as_ref().map(|w| {
            w.window_handle()
                .expect("failed to get window handle")
                .as_raw()
        });

        let context_attributes =
            glutin::context::ContextAttributesBuilder::new().build(raw_window_handle);
        let fallback_context_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_window_handle);

        // SAFETY: `raw_window_handle` came from the same window as `gl_config`; glutin requires
        // this call to create a GL context on that display.
        let not_current_gl_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_config
                        .display()
                        .create_context(&gl_config, &fallback_context_attributes)
                        .expect("failed to create context even with fallback context attributes")
                })
        };

        let window = window.take().unwrap_or_else(|| {
            glutin_winit::finalize_window(event_loop, winit_window_builder.clone(), &gl_config)
                .expect("failed to finalize glutin window")
        });

        let (width, height): (u32, u32) = window.inner_size().into();
        let width = NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN);
        let height = NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN);
        let surface_attributes =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(
                    window
                        .window_handle()
                        .expect("failed to get window handle")
                        .as_raw(),
                    width,
                    height,
                );

        // SAFETY: `surface_attributes` were built from the same `window` as `gl_config`.
        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .unwrap()
        };

        let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

        let _ = gl_surface.set_swap_interval(&gl_context, glutin::surface::SwapInterval::DontWait);

        let inner_sz = window.inner_size();
        let scale = window.scale_factor();
        debug!(
            target: "vge_embedded",
            inner_px = ?inner_sz,
            scale_factor = scale,
            outer_pos = ?window.outer_position(),
            inner_pos = ?window.inner_position(),
            "GlutinWindowContext: editor (OpenGL) window ready"
        );

        Self {
            window,
            gl_context,
            gl_display,
            gl_surface,
        }
    }

    fn window(&self) -> &winit::window::Window {
        &self.window
    }

    fn resize(&self, physical_size: winit::dpi::PhysicalSize<u32>) {
        use glutin::surface::GlSurface;
        self.gl_surface.resize(
            &self.gl_context,
            physical_size.width.try_into().unwrap_or(NonZeroU32::MIN),
            physical_size.height.try_into().unwrap_or(NonZeroU32::MIN),
        );
    }

    fn swap_buffers(&self) -> glutin::error::Result<()> {
        use glutin::surface::GlSurface;
        self.gl_surface.swap_buffers(&self.gl_context)
    }

    fn get_proc_address(&self, addr: &std::ffi::CStr) -> *const std::ffi::c_void {
        use glutin::display::GlDisplay;
        self.gl_display.get_proc_address(addr)
    }
}

/// Present one Vulkan frame to the engine window.
fn render_engine_frame(inner: &mut Inner) {
    let win = inner.engine_window.as_ref();
    let sz = win.inner_size();
    if sz.width == 0 || sz.height == 0 {
        return;
    }
    let aspect = sz.width as f32 / sz.height as f32;
    let vp = if inner.model.preview_mode_active && !inner.model.play_mode_active {
        inner.engine_state.free_view_projection(aspect)
    } else {
        inner.engine_state.view_projection(aspect)
    };
    let inst = inner.engine_state.voxel_instances_for_stream();
    // SAFETY: draw uses the initialized swapchain for `win`.
    match unsafe { inner.vk.draw_frame(&inst, vp) } {
        Ok(()) => {}
        Err(RenderError::Vulkan(code))
            if code == ash::vk::Result::ERROR_OUT_OF_DATE_KHR
                || code == ash::vk::Result::SUBOPTIMAL_KHR =>
        {
            debug!(
                target: "vge_embedded",
                ?code,
                "engine draw: swapchain OOD/SUBOPTIMAL, resizing"
            );
            if let Err(e) = unsafe { inner.vk.resize(win) } {
                error!(target: "vge_embedded", error = %e, "resize after OOD failed");
            }
        }
        Err(e) => error!(target: "vge_embedded", error = %e, "engine vk.draw_frame failed"),
    }
}

/// Advance simulation without presenting a Vulkan frame.
///
/// Used as a fallback heartbeat when the embedded engine viewport is hidden or
/// not receiving redraw events, so scripts still run during Play mode.
fn tick_engine_simulation(inner: &mut Inner) {
    let now = Instant::now();
    let dt = now
        .duration_since(inner.last_engine)
        .as_secs_f32()
        .min(0.25);
    inner.last_engine = now;
    inner.sim_accum_s += dt;
    let fixed_dt = 1.0 / 60.0;
    let steps = ((inner.sim_accum_s / fixed_dt).floor() as u32).min(5);
    if steps == 0 {
        return;
    }
    inner.sim_accum_s -= steps as f32 * fixed_dt;
    for _ in 0..steps {
        inner.engine_state.tick();
    }
}

struct Inner {
    gl_win: GlutinWindowContext,
    gl: Arc<glow::Context>,
    egui_glow: egui_glow::winit::EguiGlow,
    editor_id: WindowId,
    engine_window: Arc<Window>,
    engine_id: WindowId,
    vk: VulkanRenderer,
    model: EditorModel,
    engine_state: EngineState,
    last_engine: Instant,
    repaint_delay: std::time::Duration,
    /// Last applied engine viewport in physical pixels; used to avoid redundant `vk.resize`.
    last_engine_viewport: Option<(i32, i32, u32, u32)>,
    /// When false (fallback top-level window), viewport x/y are translated with the editor `inner_position`.
    engine_viewport_parent_relative: bool,
    /// Log once when `engine_viewport_px` is `None` so logs are not spammed every frame.
    logged_missing_engine_viewport: bool,
    /// After `paint_engine_frame` from the editor `RedrawRequested` path (e.g. Play), skip one
    /// redundant engine-window redraw so we do not tick/present twice in the same turn.
    skip_next_engine_redraw_paint: bool,
    /// True when engine window currently has cursor/input capture.
    engine_input_captured: bool,
    debug_frame_counter: u64,
    fps_window_start: Instant,
    fps_window_frames: u32,
    sim_accum_s: f32,
    vsync_enabled_applied: bool,
    device_events_always: bool,
    preview_orbiting: bool,
    preview_panning: bool,
    preview_last_cursor: Option<(f64, f64)>,
}

fn set_engine_input_capture(inner: &mut Inner, capture: bool) {
    let win = inner.engine_window.as_ref();
    if capture {
        win.focus_window();
        // Some platforms/window modes (especially embedded/child windows) may
        // support one grab mode but not the other; try both best-effort.
        let locked_ok = win.set_cursor_grab(CursorGrabMode::Locked).is_ok();
        let confined_ok = win.set_cursor_grab(CursorGrabMode::Confined).is_ok();
        win.set_cursor_visible(false);
        inner.engine_input_captured = locked_ok || confined_ok;
        debug!(target: "vge_embedded", "engine input capture ENABLED");
    } else {
        let _ = win.set_cursor_grab(CursorGrabMode::None);
        win.set_cursor_visible(true);
        inner.engine_input_captured = false;
        debug!(target: "vge_embedded", "engine input capture DISABLED");
    }
}

fn set_device_event_mode(event_loop: &ActiveEventLoop, inner: &mut Inner, always: bool) {
    if inner.device_events_always == always {
        return;
    }
    event_loop.listen_device_events(if always {
        DeviceEvents::Always
    } else {
        DeviceEvents::WhenFocused
    });
    inner.device_events_always = always;
}

fn apply_movement_key(inner: &mut Inner, event: &winit::event::KeyEvent) {
    let down = event.state.is_pressed();
    match event.physical_key {
        PhysicalKey::Code(KeyCode::KeyW) => inner.engine_state.set_key_down("w", down),
        PhysicalKey::Code(KeyCode::KeyA) => inner.engine_state.set_key_down("a", down),
        PhysicalKey::Code(KeyCode::KeyS) => inner.engine_state.set_key_down("s", down),
        PhysicalKey::Code(KeyCode::KeyD) => inner.engine_state.set_key_down("d", down),
        PhysicalKey::Code(KeyCode::Space) => inner.engine_state.set_key_down("space", down),
        PhysicalKey::Code(KeyCode::ShiftLeft) | PhysicalKey::Code(KeyCode::ShiftRight) => {
            inner.engine_state.set_key_down("shift", down)
        }
        _ => {}
    }
}

fn preview_camera_forward(yaw: f32, pitch: f32) -> glam::Vec3 {
    let (sy, cy) = pitch.sin_cos();
    let (sx, cx) = yaw.sin_cos();
    glam::Vec3::new(cx * cy, sy, sx * cy).normalize_or_zero()
}

fn note_engine_present(inner: &mut Inner) {
    inner.fps_window_frames = inner.fps_window_frames.saturating_add(1);
    let elapsed = inner.fps_window_start.elapsed();
    if elapsed.as_secs_f32() >= 0.5 {
        let secs = elapsed.as_secs_f32().max(0.001);
        inner.model.render_fps = inner.fps_window_frames as f32 / secs;
        inner.fps_window_start = Instant::now();
        inner.fps_window_frames = 0;
    }
}

fn apply_script_cursor_commands(inner: &mut Inner) {
    let cmds = inner.engine_state.take_cursor_commands();
    if let Some(visible) = cmds.cursor_visible {
        inner.engine_window.set_cursor_visible(visible);
    }
    if cmds.center_mouse {
        let sz = inner.engine_window.inner_size();
        if sz.width > 0 && sz.height > 0 {
            let center = PhysicalPosition::new((sz.width / 2) as f64, (sz.height / 2) as f64);
            let _ = inner.engine_window.set_cursor_position(center);
            inner.engine_state.last_cursor_pos = None;
        }
    }
}

fn recenter_engine_cursor(inner: &mut Inner) {
    let sz = inner.engine_window.inner_size();
    if sz.width == 0 || sz.height == 0 {
        return;
    }
    let center = PhysicalPosition::new((sz.width / 2) as f64, (sz.height / 2) as f64);
    let _ = inner.engine_window.set_cursor_position(center);
    // Avoid synthetic warp deltas affecting script look calculations.
    inner.engine_state.last_cursor_pos = None;
}

struct EmbeddedApp {
    port: u16,
    proxy: winit::event_loop::EventLoopProxy<UserEvent>,
    inner: Option<Inner>,
}

impl EmbeddedApp {
    fn new(port: u16, proxy: winit::event_loop::EventLoopProxy<UserEvent>) -> Self {
        Self {
            port,
            proxy,
            inner: None,
        }
    }
}

impl ApplicationHandler<UserEvent> for EmbeddedApp {
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        let Some(inner) = &mut self.inner else {
            return;
        };
        if let DeviceEvent::MouseMotion { delta } = event {
            if inner.model.play_mode_active && inner.engine_input_captured {
                inner.engine_state.on_mouse_motion(delta.0, delta.1);
                debug!(
                    target: "vge_embedded",
                    dx = delta.0,
                    dy = delta.1,
                    "raw mouse motion (device event)"
                );
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.listen_device_events(DeviceEvents::WhenFocused);
        debug!(
            target: "vge_embedded",
            "device event mode set to WhenFocused"
        );
        // SAFETY: `resumed` runs on the winit thread; GL context is created before use below.
        let gl_win = unsafe { GlutinWindowContext::new(event_loop) };
        let editor_id = gl_win.window().id();
        // SAFETY: `get_proc_address` uses the active GL display from `gl_win`.
        let gl = Arc::new(unsafe {
            glow::Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s)
                    .expect("failed to construct C string from string for gl proc address");
                gl_win.get_proc_address(&s)
            })
        });
        gl_win.window().set_visible(true);

        let egui_glow = egui_glow::winit::EguiGlow::new(event_loop, gl.clone(), None, None, true);
        let proxy = self.proxy.clone();
        egui_glow
            .egui_ctx
            .set_request_repaint_callback(move |info| {
                let _ = proxy.send_event(UserEvent::Redraw(info.delay));
            });

        let engine_base = WindowAttributes::default()
            .with_title("Engine view (embedded)")
            .with_decorations(false)
            .with_inner_size(LogicalSize {
                width: 800.0,
                height: 480.0,
            });

        // Child window (`with_parent_window`): WS_CHILD on Windows / X11 reparent so the view lives
        // in the editor's client area (not a separate top-level stack). If creation fails, fall
        // back to a free top-level window (Windows: owned by editor HWND so it still tracks the app).
        let (engine_window, engine_viewport_parent_relative) = {
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                let parent = gl_win
                    .window()
                    .window_handle()
                    .expect("parent window handle")
                    .as_raw();
                debug!(
                    target: "vge_embedded",
                    parent_handle = ?parent,
                    "creating engine window with parent (WS_CHILD / X11 embed when supported)"
                );
                // SAFETY: `parent` is the editor window handle; winit documents this for child surfaces.
                let attrs = unsafe { engine_base.clone().with_parent_window(Some(parent)) };
                match event_loop.create_window(attrs) {
                    Ok(w) => {
                        info!(
                            target: "vge_embedded",
                            engine_inner = ?w.inner_size(),
                            engine_scale = w.scale_factor(),
                            "engine window: created as child (viewport uses parent client coordinates)"
                        );
                        (Arc::new(w), true)
                    }
                    Err(e) => {
                        warn!(
                            target: "vge_embedded",
                            error = %e,
                            "embedded child window failed; using fallback engine window"
                        );
                        #[cfg(target_os = "windows")]
                        {
                            use winit::platform::windows::WindowAttributesExtWindows;
                            use winit::raw_window_handle::RawWindowHandle;
                            let mut fb = engine_base.clone();
                            let raw = gl_win
                                .window()
                                .window_handle()
                                .expect("editor window handle")
                                .as_raw();
                            if let RawWindowHandle::Win32(h) = raw {
                                fb = fb.with_owner_window(h.hwnd.get());
                                debug!(
                                    target: "vge_embedded",
                                    owner_hwnd = h.hwnd.get(),
                                    "Windows fallback: engine window with owner (not WS_CHILD)"
                                );
                            }
                            let w = event_loop.create_window(fb).expect("engine window");
                            info!(
                                target: "vge_embedded",
                                engine_inner = ?w.inner_size(),
                                "engine window: fallback top-level frameless (position uses editor inner_position + egui rect)"
                            );
                            (Arc::new(w), false)
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            (
                                Arc::new(
                                    event_loop
                                        .create_window(engine_base)
                                        .expect("engine window"),
                                ),
                                false,
                            )
                        }
                    }
                }
            }
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            {
                let w = event_loop
                    .create_window(engine_base)
                    .expect("engine window");
                info!(
                    target: "vge_embedded",
                    engine_inner = ?w.inner_size(),
                    "engine window: top-level (no parent embed on this platform)"
                );
                (Arc::new(w), false)
            }
        };

        let engine_id = engine_window.id();
        debug!(
            target: "vge_embedded",
            engine_id = ?engine_id,
            parent_relative_coords = engine_viewport_parent_relative,
            "engine window handle ready for Vulkan surface"
        );
        // SAFETY: `engine_window` is a valid Vulkan `HasWindowHandle` surface target.
        let mut vk = match unsafe { VulkanRenderer::new(engine_window.as_ref()) } {
            Ok(r) => {
                info!(target: "vge_embedded", "VulkanRenderer initialized for embedded engine window");
                r
            }
            Err(e) => {
                error!(target: "vge_embedded", error = %e, "Vulkan init failed for embedded engine window");
                event_loop.exit();
                return;
            }
        };

        let mut model = EditorModel::new(self.port);
        crate::editor_state::apply_loaded_session(&mut model);
        let project_vsync = model
            .current_project
            .as_ref()
            .map(|p| p.vsync_enabled)
            .unwrap_or(false);
        // SAFETY: renderer is initialized and bound to this engine window.
        if let Err(e) = unsafe { vk.set_vsync_enabled(engine_window.as_ref(), project_vsync) } {
            warn!(
                target: "vge_embedded",
                error = %e,
                "failed to apply initial project VSync"
            );
        }
        model.push_log(
            "Embedded editor: use Play or File to run; the 3D view follows the central viewport.",
        );

        self.inner = Some(Inner {
            gl_win,
            gl,
            egui_glow,
            editor_id,
            engine_window,
            engine_id,
            vk,
            model,
            engine_state: EngineState::default(),
            last_engine: Instant::now(),
            repaint_delay: std::time::Duration::MAX,
            last_engine_viewport: None,
            engine_viewport_parent_relative,
            logged_missing_engine_viewport: false,
            skip_next_engine_redraw_paint: false,
            engine_input_captured: false,
            debug_frame_counter: 0,
            fps_window_start: Instant::now(),
            fps_window_frames: 0,
            sim_accum_s: 0.0,
            vsync_enabled_applied: project_vsync,
            device_events_always: false,
            preview_orbiting: false,
            preview_panning: false,
            preview_last_cursor: None,
        });

        if let Some(i) = &self.inner {
            i.gl_win.window().request_redraw();
            i.engine_window.request_redraw();
        }
        info!(
            target: "vge_embedded",
            editor_id = ?editor_id,
            engine_id = ?engine_id,
            child_or_embed = engine_viewport_parent_relative,
            "embedded editor windows created; set RUST_LOG=vge_embedded=debug for viewport traces"
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(inner) = &mut self.inner else {
            return;
        };

        if matches!(event, WindowEvent::CloseRequested | WindowEvent::Destroyed) {
            event_loop.exit();
            return;
        }

        if window_id == inner.editor_id {
            if let WindowEvent::Resized(physical_size) = &event {
                inner.gl_win.resize(*physical_size);
            }

            if matches!(event, WindowEvent::RedrawRequested) {
                let model_ptr: *mut EditorModel = &mut inner.model;
                let es_ptr: *mut EngineState = &mut inner.engine_state;
                let w = inner.gl_win.window();
                inner.egui_glow.run(w, |egui_ctx| {
                    // SAFETY: `run` is synchronous; pointers refer to `inner` fields and are not
                    // used after this closure returns. Same thread as the event loop.
                    unsafe {
                        draw_editor_ui(egui_ctx, &mut *model_ptr, Some(&mut *es_ptr));
                    }
                });

                if inner.model.preview_mode_active && !inner.model.play_mode_active {
                    // Keep editor edits visible in embedded viewport without requiring Play.
                    let level = inner.model.level.clone();
                    let asset_root = inner.model.project_root_dir();
                    let saved_cam = (
                        inner.engine_state.camera_pos,
                        inner.engine_state.yaw,
                        inner.engine_state.pitch,
                    );
                    inner
                        .engine_state
                        .apply_level_with_asset_root(&level, asset_root.as_deref());
                    inner.engine_state.camera_pos = saved_cam.0;
                    inner.engine_state.yaw = saved_cam.1;
                    inner.engine_state.pitch = saved_cam.2;
                }

                // Apply play-mode input capture transitions requested by UI state.
                if inner.model.play_mode_capture_request {
                    set_engine_input_capture(inner, true);
                    inner.model.play_mode_capture_request = false;
                } else if inner.engine_input_captured != inner.model.play_mode_active {
                    set_engine_input_capture(inner, inner.model.play_mode_active);
                }
                let project_vsync = inner
                    .model
                    .current_project
                    .as_ref()
                    .map(|p| p.vsync_enabled)
                    .unwrap_or(false);
                if project_vsync != inner.vsync_enabled_applied {
                    // SAFETY: same renderer/window pair; this recreates swapchain resources.
                    match unsafe {
                        inner
                            .vk
                            .set_vsync_enabled(inner.engine_window.as_ref(), project_vsync)
                    } {
                        Ok(()) => {
                            inner.vsync_enabled_applied = project_vsync;
                            inner.model.push_log(if project_vsync {
                                "Project setting: VSync enabled."
                            } else {
                                "Project setting: VSync disabled (uncapped)."
                            });
                        }
                        Err(e) => inner
                            .model
                            .push_log(format!("Failed to apply project VSync: {e}")),
                    }
                }

                {
                    let eng = inner.engine_window.clone();
                    let parent_rel = inner.engine_viewport_parent_relative;
                    let editor = inner.gl_win.window();
                    if let Some((x, y, w, h)) = inner.model.engine_viewport_px {
                        inner.logged_missing_engine_viewport = false;
                        if w >= 1 && h >= 1 {
                            eng.set_visible(true);
                            let pos = if parent_rel {
                                // Child window: (x, y) are client-relative to the parent (editor).
                                PhysicalPosition::new(x, y)
                            } else if let Ok(ip) = editor.inner_position() {
                                // Fallback top-level frameless window: egui rects are relative to the
                                // editor *client* area; `outer_position` would shift the view by the
                                // title bar / frame (appearing "detached").
                                PhysicalPosition::new(ip.x + x, ip.y + y)
                            } else {
                                debug!(
                                    target: "vge_embedded",
                                    "editor.inner_position() failed; using raw (x,y) for engine position (may be wrong)"
                                );
                                PhysicalPosition::new(x, y)
                            };
                            eng.set_outer_position(pos);
                            let _ = eng.request_inner_size(PhysicalSize::new(w, h));
                            let vp = (x, y, w, h);
                            if inner.last_engine_viewport != Some(vp) {
                                inner.last_engine_viewport = Some(vp);
                                debug!(
                                    target: "vge_embedded",
                                    viewport_px = ?vp,
                                    parent_relative = parent_rel,
                                    editor_inner_pos = ?editor.inner_position(),
                                    engine_pos_applied = ?pos,
                                    main_tab = ?inner.model.main_tab,
                                    "applied engine viewport (egui → winit)"
                                );
                                // Use egui (w,h) — `request_inner_size` is async; `resize` would read stale `inner_size`.
                                // SAFETY: `resize_to` is the Vulkan swapchain path for this window.
                                if let Err(e) = unsafe { inner.vk.resize_to(eng.as_ref(), w, h) } {
                                    error!(target: "vge_embedded", error = %e, "viewport vk.resize_to failed");
                                }
                            }
                        } else {
                            eng.set_visible(false);
                            inner.last_engine_viewport = None;
                            debug!(
                                target: "vge_embedded",
                                w,
                                h,
                                "engine view hidden: viewport size too small"
                            );
                        }
                    } else {
                        if !inner.logged_missing_engine_viewport {
                            inner.logged_missing_engine_viewport = true;
                            debug!(
                                target: "vge_embedded",
                                main_tab = ?inner.model.main_tab,
                                "engine_viewport_px is None — engine window hidden (use Level tab, or layout not ready yet)"
                            );
                        }
                        eng.set_visible(false);
                        inner.last_engine_viewport = None;
                    }
                }

                // After Play, `apply_level` runs inside egui; winit may deliver engine
                // `RedrawRequested` before this editor handler finishes — present one Vulkan frame
                // here so the 3D view updates immediately (same frame as the UI).
                if inner.model.pending_engine_repaint {
                    inner.repaint_delay = std::time::Duration::ZERO;
                    render_engine_frame(inner);
                    note_engine_present(inner);
                    inner.model.pending_engine_repaint = false;
                    inner.skip_next_engine_redraw_paint = true;
                    inner.egui_glow.egui_ctx.request_repaint();
                }
                for line in inner.engine_state.drain_script_logs() {
                    inner.model.push_log(format!("[lua] {line}"));
                }
                inner.debug_frame_counter = inner.debug_frame_counter.wrapping_add(1);
                if inner.model.play_mode_active && inner.debug_frame_counter % 120 == 0 {
                    debug!(
                        target: "vge_embedded",
                        frame = inner.debug_frame_counter,
                        captured = inner.engine_input_captured,
                        mouse_delta = ?inner.engine_state.mouse_delta,
                        "embedded play heartbeat"
                    );
                }

                // SAFETY: `gl` is bound to the current GL context for this window.
                unsafe {
                    use glow::HasContext as _;
                    inner.gl.clear_color(0.12, 0.12, 0.14, 1.0);
                    inner.gl.clear(glow::COLOR_BUFFER_BIT);
                }

                inner.egui_glow.paint(inner.gl_win.window());
                let _ = inner.gl_win.swap_buffers();
                inner.engine_window.request_redraw();

                event_loop.set_control_flow(if inner.model.play_mode_active {
                    inner.gl_win.window().request_redraw();
                    inner.engine_window.request_redraw();
                    ControlFlow::Poll
                } else if inner.repaint_delay.is_zero() {
                    inner.gl_win.window().request_redraw();
                    ControlFlow::Poll
                } else if let Some(t) = std::time::Instant::now().checked_add(inner.repaint_delay) {
                    ControlFlow::WaitUntil(t)
                } else {
                    ControlFlow::Wait
                });

                return;
            }

            if let WindowEvent::KeyboardInput { event, .. } = &event {
                apply_movement_key(inner, event);
                if event.state.is_pressed()
                    && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                    && (inner.model.play_mode_active || inner.engine_input_captured)
                {
                    inner.model.stop_play_mode("Play mode stopped (Esc).");
                    set_engine_input_capture(inner, false);
                    inner.gl_win.window().request_redraw();
                    inner.engine_window.request_redraw();
                    return;
                }
            }

            let response = inner
                .egui_glow
                .on_window_event(inner.gl_win.window(), &event);
            if response.repaint {
                inner.gl_win.window().request_redraw();
            }
            return;
        }

        if window_id == inner.engine_id {
            let win = inner.engine_window.clone();
            match event {
                WindowEvent::Resized(ps) => {
                    debug!(
                        target: "vge_embedded",
                        ?ps,
                        inner = ?win.inner_size(),
                        visible = ?win.is_visible(),
                        "engine window Resized"
                    );
                    // SAFETY: Vulkan swapchain resize for the engine surface.
                    if let Err(e) = unsafe { inner.vk.resize(win.as_ref()) } {
                        error!(target: "vge_embedded", error = %e, "engine vk.resize after Resized failed");
                    }
                    win.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    if inner.model.play_mode_active {
                        // Play mode uses a direct heartbeat in `about_to_wait` for both
                        // simulation and rendering. Avoid double-presenting here.
                        event_loop.set_control_flow(ControlFlow::Poll);
                        return;
                    }
                    let sz = win.inner_size();
                    if sz.width > 0 && sz.height > 0 {
                        if inner.skip_next_engine_redraw_paint {
                            inner.skip_next_engine_redraw_paint = false;
                            debug!(
                                target: "vge_embedded",
                                "engine RedrawRequested skipped (already painted after Play in editor pass)"
                            );
                        } else {
                            render_engine_frame(inner);
                            note_engine_present(inner);
                        }
                    } else {
                        debug!(
                            target: "vge_embedded",
                            ?sz,
                            "engine RedrawRequested skipped: zero inner size"
                        );
                    }
                    inner.gl_win.window().request_redraw();
                    win.request_redraw();
                    if inner.model.play_mode_active {
                        event_loop.set_control_flow(ControlFlow::Poll);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    inner.engine_state.on_cursor_moved(position.x, position.y);
                    if inner.model.preview_mode_active && !inner.model.play_mode_active {
                        if let Some((lx, ly)) = inner.preview_last_cursor {
                            let dx = (position.x - lx) as f32;
                            let dy = (position.y - ly) as f32;
                            if inner.preview_orbiting {
                                inner.engine_state.yaw += dx * 0.005;
                                inner.engine_state.pitch =
                                    (inner.engine_state.pitch - dy * 0.005).clamp(-1.5, 1.5);
                                inner.gl_win.window().request_redraw();
                            } else if inner.preview_panning {
                                let f = preview_camera_forward(
                                    inner.engine_state.yaw,
                                    inner.engine_state.pitch,
                                );
                                let right = f.cross(glam::Vec3::Y).normalize_or_zero();
                                let up = glam::Vec3::Y;
                                let pan_speed = 0.02;
                                inner.engine_state.camera_pos -= right * dx * pan_speed;
                                inner.engine_state.camera_pos += up * dy * pan_speed;
                                inner.gl_win.window().request_redraw();
                            }
                        }
                        inner.preview_last_cursor = Some((position.x, position.y));
                    }
                    if inner.model.play_mode_active {
                        debug!(
                            target: "vge_embedded",
                            x = position.x,
                            y = position.y,
                            "engine cursor moved (window event)"
                        );
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    if inner.model.preview_mode_active && !inner.model.play_mode_active {
                        match button {
                            MouseButton::Middle => {
                                inner.preview_orbiting = state.is_pressed();
                                if !inner.preview_orbiting {
                                    inner.preview_last_cursor = None;
                                }
                            }
                            MouseButton::Right => {
                                inner.preview_panning = state.is_pressed();
                                if !inner.preview_panning {
                                    inner.preview_last_cursor = None;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    if inner.model.preview_mode_active && !inner.model.play_mode_active {
                        let scroll = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y,
                            MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.05,
                        };
                        let f = preview_camera_forward(
                            inner.engine_state.yaw,
                            inner.engine_state.pitch,
                        );
                        inner.engine_state.camera_pos += f * scroll * 1.5;
                        inner.gl_win.window().request_redraw();
                    }
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    apply_movement_key(inner, &event);
                    if event.state.is_pressed()
                        && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                        && (inner.model.play_mode_active || inner.engine_input_captured)
                    {
                        inner.model.stop_play_mode("Play mode stopped (Esc).");
                        set_engine_input_capture(inner, false);
                        inner.gl_win.window().request_redraw();
                    }
                }
                _ => {}
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        if let Some(inner) = &mut self.inner {
            match event {
                UserEvent::Redraw(delay) => {
                    inner.repaint_delay = if inner.model.pending_engine_repaint {
                        inner.model.pending_engine_repaint = false;
                        std::time::Duration::ZERO
                    } else {
                        delay
                    };
                    inner.gl_win.window().request_redraw();
                    inner.engine_window.request_redraw();
                    event_loop.set_control_flow(ControlFlow::Poll);
                }
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let winit::event::StartCause::ResumeTimeReached { .. } = &cause {
            if let Some(inner) = &self.inner {
                inner.gl_win.window().request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(inner) = &mut self.inner else {
            return;
        };

        if inner.model.play_mode_active {
            set_device_event_mode(event_loop, inner, true);
            // Keep capture sticky during play even if the OS/focus changes.
            if !inner.engine_input_captured {
                set_engine_input_capture(inner, true);
            }
            // Embedded child windows can lose confinement on some platforms;
            // enforce hidden + centered cursor continuously during play.
            inner.engine_window.set_cursor_visible(false);
            recenter_engine_cursor(inner);
            tick_engine_simulation(inner);
            apply_script_cursor_commands(inner);
            for line in inner.engine_state.drain_script_logs() {
                inner.model.push_log(format!("[lua] {line}"));
            }
            // Keep both windows pumping while in Play/Capture mode, even if no
            // external window events are arriving from the OS.
            if inner.engine_window.is_visible().unwrap_or(true) {
                render_engine_frame(inner);
                note_engine_present(inner);
                inner.engine_window.request_redraw();
            }
            inner.gl_win.window().request_redraw();
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }
        set_device_event_mode(event_loop, inner, false);

        if inner.model.preview_mode_active {
            inner.gl_win.window().request_redraw();
            if inner.engine_window.is_visible().unwrap_or(true) {
                render_engine_frame(inner);
                note_engine_present(inner);
                inner.engine_window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }

        if inner.repaint_delay.is_zero() {
            inner.gl_win.window().request_redraw();
            event_loop.set_control_flow(ControlFlow::Poll);
        } else if let Some(t) = std::time::Instant::now().checked_add(inner.repaint_delay) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(t));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(mut inner) = self.inner.take() {
            set_engine_input_capture(&mut inner, false);
            if let Err(e) = crate::editor_state::save_from_model(&inner.model) {
                warn!("failed to save editor session: {e}");
            }
            inner.egui_glow.destroy();
        }
    }
}

pub fn run_embedded(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        target: "vge_embedded",
        port,
        "run_embedded: building event loop (tip: RUST_LOG=vge_embedded=debug for detailed traces)"
    );
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let mut app = EmbeddedApp::new(port, proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}
