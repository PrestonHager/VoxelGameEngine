//! Minimal game host: Vulkan + ECS + voxel stream sampling.

mod ipc;

use engine_core::EngineState;
use render_vulkan::{RenderError, VulkanRenderer};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

fn main() {
    logging::init();
    info!("engine-runner starting");

    let (ipc_tx, ipc_rx) = channel();
    if let Ok(port_s) = std::env::var("VGE_IPC_PORT") {
        if let Ok(port) = port_s.parse::<u16>() {
            ipc::spawn_listener(port, ipc_tx);
        }
    }

    let event_loop = match EventLoop::new() {
        Ok(e) => e,
        Err(e) => {
            error!("event loop: {e}");
            return;
        }
    };

    let mut app = RunnerApp {
        window: None,
        renderer: None,
        state: EngineState::default(),
        last: Instant::now(),
        spin: 0.0f32,
        ipc_rx,
    };

    if let Err(e) = event_loop.run_app(&mut app) {
        error!("run_app: {e}");
    }
}

struct RunnerApp {
    window: Option<Arc<Window>>,
    renderer: Option<VulkanRenderer>,
    state: EngineState,
    last: Instant,
    spin: f32,
    ipc_rx: std::sync::mpsc::Receiver<ipc::EngineIpcOp>,
}

impl ApplicationHandler for RunnerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = platform::default_window_attributes();
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        self.window = Some(window.clone());
        match unsafe { VulkanRenderer::new(window.as_ref()) } {
            Ok(r) => {
                self.renderer = Some(r);
                window.request_redraw();
            }
            Err(e) => {
                error!("Vulkan init failed: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let window = self.window.as_ref().expect("window");
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                if let Some(r) = self.renderer.as_mut() {
                    if let Err(e) = unsafe { r.resize(window.as_ref()) } {
                        error!("resize: {e}");
                    }
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = now.duration_since(self.last).as_secs_f32();
                self.last = now;
                self.spin += dt * 0.7;

                while let Ok(op) = self.ipc_rx.try_recv() {
                    match op {
                        ipc::EngineIpcOp::LoadLevelFromPath(path) => {
                            match std::fs::read_to_string(&path) {
                                Ok(s) => match scene::Level::from_json_str(&s) {
                                    Ok(level) => {
                                        self.state.apply_level(&level);
                                        info!("Loaded level from {path}");
                                    }
                                    Err(e) => error!("level JSON {path}: {e}"),
                                },
                                Err(e) => error!("read level file {path}: {e}"),
                            }
                        }
                    }
                }

                let steps = ((dt * 60.0).floor() as u32).clamp(1, 5);
                for _ in 0..steps {
                    self.state.tick();
                }

                let sz = window.inner_size();
                let aspect = sz.width.max(1) as f32 / sz.height.max(1) as f32;
                let mut vp = self.state.view_projection(aspect);
                // gentle orbit wobble
                vp *= glam::Mat4::from_rotation_y(self.spin * 0.02);

                let inst = self.state.voxel_instances_for_stream();

                if let Some(r) = self.renderer.as_mut() {
                    match unsafe { r.draw_frame(&inst, vp) } {
                        Ok(()) => {}
                        Err(RenderError::Vulkan(code))
                            if code == ash::vk::Result::ERROR_OUT_OF_DATE_KHR
                                || code == ash::vk::Result::SUBOPTIMAL_KHR =>
                        {
                            if let Err(e) = unsafe { r.resize(window.as_ref()) } {
                                error!("resize after OOD: {e}");
                            }
                        }
                        Err(e) => error!("draw: {e}"),
                    }
                }
                window.request_redraw();
            }
            _ => {}
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}
}
