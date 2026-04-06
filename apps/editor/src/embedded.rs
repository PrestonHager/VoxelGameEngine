//! In-process editor + Vulkan using winit child window (embedded view).
//!
//! Set `VGE_EMBEDDED=1` to use this path instead of eframe + external `engine-runner`.

#![allow(unsafe_code)]

use crate::model::EditorModel;
use crate::ui::draw_editor_ui;
use egui_winit::winit;
use engine_core::EngineState;
use render_vulkan::{RenderError, VulkanRenderer};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::HasWindowHandle;
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
    unsafe fn new(event_loop: &ActiveEventLoop) -> Self {
        use glutin::context::NotCurrentGlContext;
        use glutin::display::GetGlDisplay;
        use glutin::display::GlDisplay;
        use glutin::prelude::GlSurface;

        let winit_window_builder = winit::window::WindowAttributes::default()
            .with_resizable(true)
            .with_inner_size(LogicalSize {
                width: 1200.0,
                height: 780.0,
            })
            .with_title("Voxel Editor")
            .with_visible(false);

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
                    config_iterator
                        .next()
                        .expect("failed to find a matching configuration for creating glutin config")
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

        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .unwrap()
        };

        let gl_context = not_current_gl_context.make_current(&gl_surface).unwrap();

        let _ = gl_surface.set_swap_interval(
            &gl_context,
            glutin::surface::SwapInterval::Wait(NonZeroU32::MIN),
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
    /// When false (fallback top-level window), viewport x/y are translated with the editor `outer_position`.
    engine_viewport_parent_relative: bool,
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
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let gl_win = unsafe { GlutinWindowContext::new(event_loop) };
        let editor_id = gl_win.window().id();
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
        egui_glow.egui_ctx.set_request_repaint_callback(move |info| {
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
                let attrs = unsafe { engine_base.clone().with_parent_window(Some(parent)) };
                match event_loop.create_window(attrs) {
                    Ok(w) => (Arc::new(w), true),
                    Err(e) => {
                        warn!("embedded child window failed ({e}); using fallback engine window");
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
                            }
                            (
                                Arc::new(
                                    event_loop
                                        .create_window(fb)
                                        .expect("engine window"),
                                ),
                                false,
                            )
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
                (
                    Arc::new(
                        event_loop
                            .create_window(engine_base)
                            .expect("engine window"),
                    ),
                    false,
                )
            }
        };

        let engine_id = engine_window.id();
        let vk = match unsafe { VulkanRenderer::new(engine_window.as_ref()) } {
            Ok(r) => r,
            Err(e) => {
                error!("Vulkan init failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let mut model = EditorModel::new(self.port);
        crate::editor_state::apply_loaded_session(&mut model);
        model.push_log("Embedded editor: use Play or File to run; the 3D view follows the central viewport.");

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
        });

        if let Some(i) = &self.inner {
            i.gl_win.window().request_redraw();
            i.engine_window.request_redraw();
        }
        info!("embedded editor: editor={editor_id:?} engine={engine_id:?}");
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
                // SAFETY: `run` is synchronous; pointers are only dereferenced inside this closure.
                inner.egui_glow.run(w, |egui_ctx| unsafe {
                    draw_editor_ui(egui_ctx, &mut *model_ptr, Some(&mut *es_ptr));
                });

                if inner.model.pending_engine_repaint {
                    // egui may queue `UserEvent::Redraw` with a delay after this; `user_event` forces zero
                    // delay when `pending_engine_repaint` so we don't fall back to `WaitUntil` before the engine paints.
                    inner.repaint_delay = std::time::Duration::ZERO;
                }

                {
                    let eng = inner.engine_window.clone();
                    let parent_rel = inner.engine_viewport_parent_relative;
                    let editor = inner.gl_win.window();
                    if let Some((x, y, w, h)) = inner.model.engine_viewport_px {
                        if w >= 1 && h >= 1 {
                            eng.set_visible(true);
                            let pos = if parent_rel {
                                PhysicalPosition::new(x, y)
                            } else if let Ok(origin) = editor.outer_position() {
                                // Fallback top-level: rough offset (egui rect is client-relative).
                                PhysicalPosition::new(origin.x + x, origin.y + y)
                            } else {
                                PhysicalPosition::new(x, y)
                            };
                            eng.set_outer_position(pos);
                            let _ = eng.request_inner_size(PhysicalSize::new(w, h));
                            let vp = (x, y, w, h);
                            if inner.last_engine_viewport != Some(vp) {
                                inner.last_engine_viewport = Some(vp);
                                // Use egui (w,h) — `request_inner_size` is async; `resize` would read stale `inner_size`.
                                if let Err(e) = unsafe { inner.vk.resize_to(eng.as_ref(), w, h) } {
                                    error!("viewport resize: {e}");
                                }
                            }
                        } else {
                            eng.set_visible(false);
                            inner.last_engine_viewport = None;
                        }
                    } else {
                        eng.set_visible(false);
                        inner.last_engine_viewport = None;
                    }
                }

                unsafe {
                    use glow::HasContext as _;
                    inner.gl.clear_color(0.12, 0.12, 0.14, 1.0);
                    inner.gl.clear(glow::COLOR_BUFFER_BIT);
                }

                inner.egui_glow.paint(inner.gl_win.window());
                let _ = inner.gl_win.swap_buffers();
                inner.engine_window.request_redraw();

                event_loop.set_control_flow(if inner.repaint_delay.is_zero() {
                    inner.gl_win.window().request_redraw();
                    ControlFlow::Poll
                } else if let Some(t) =
                    std::time::Instant::now().checked_add(inner.repaint_delay)
                {
                    ControlFlow::WaitUntil(t)
                } else {
                    ControlFlow::Wait
                });

                return;
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
                WindowEvent::Resized(_) => {
                    if let Err(e) = unsafe { inner.vk.resize(win.as_ref()) } {
                        error!("resize: {e}");
                    }
                    win.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let now = Instant::now();
                    let dt = now.duration_since(inner.last_engine).as_secs_f32();
                    inner.last_engine = now;

                    let steps = ((dt * 60.0).floor() as u32).clamp(1, 5);
                    for _ in 0..steps {
                        inner.engine_state.tick();
                    }

                    let sz = win.inner_size();
                    if sz.width > 0 && sz.height > 0 {
                        let aspect = sz.width as f32 / sz.height as f32;
                        let vp = inner.engine_state.view_projection(aspect);
                        let inst = inner.engine_state.voxel_instances_for_stream();

                        match unsafe { inner.vk.draw_frame(&inst, vp) } {
                            Ok(()) => {}
                            Err(RenderError::Vulkan(code))
                                if code == ash::vk::Result::ERROR_OUT_OF_DATE_KHR
                                    || code == ash::vk::Result::SUBOPTIMAL_KHR =>
                            {
                                if let Err(e) = unsafe { inner.vk.resize(win.as_ref()) } {
                                    error!("resize after OOD: {e}");
                                }
                            }
                            Err(e) => error!("draw: {e}"),
                        }
                    }
                    inner.gl_win.window().request_redraw();
                    win.request_redraw();
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

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(mut inner) = self.inner.take() {
            if let Err(e) = crate::editor_state::save_from_model(&inner.model) {
                warn!("failed to save editor session: {e}");
            }
            inner.egui_glow.destroy();
        }
    }
}

pub fn run_embedded(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let mut app = EmbeddedApp::new(port, proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}
