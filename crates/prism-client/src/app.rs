// SPDX-License-Identifier: AGPL-3.0-or-later
//! Main application — winit event loop, wgpu renderer, UI state machine.

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::config::ClientConfig;
use crate::input::InputCoalescer;
use crate::input::double_tap::DoubleTapDetector;
use crate::renderer::PrismRenderer;
use crate::session_bridge::SessionBridge;
use crate::ui::UiState;
use crate::ui::widgets::PaintContext;

/// Top-level PRISM application — owns the winit window, wgpu renderer, and UI state.
#[allow(dead_code)]
pub struct PrismApp {
    config: ClientConfig,
    window: Option<Arc<Window>>,
    renderer: Option<PrismRenderer>,
    ui_state: UiState,
    double_tap: DoubleTapDetector,
    coalescer: InputCoalescer,
    paint_ctx: PaintContext,
    bridge: SessionBridge,
}

impl PrismApp {
    pub fn new(config: ClientConfig) -> Self {
        let ui_state = UiState::initial(config.launch_mode);
        Self {
            config,
            window: None,
            renderer: None,
            ui_state,
            double_tap: DoubleTapDetector::new(std::time::Duration::from_millis(300)),
            coalescer: InputCoalescer::new(),
            paint_ctx: PaintContext::new(),
            bridge: SessionBridge::new(),
        }
    }

    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
        let mut app = self;
        event_loop.run_app(&mut app)?;
        Ok(())
    }

    fn render(&mut self) {
        let renderer = match &self.renderer {
            Some(r) => r,
            None => return,
        };

        let output = match renderer.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                renderer
                    .surface
                    .configure(&renderer.device, &renderer.surface_config);
                return;
            }
            Err(e) => {
                tracing::error!("Surface error: {e}");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = renderer
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        // Clear to the deep purple background
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.051, // #0d = 13/255
                            g: 0.043, // #0b = 11/255
                            b: 0.102, // #1a = 26/255
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            // Pass drops, clearing the screen
        }

        // TODO: render stream texture, blur, glass panels, text based on self.ui_state

        renderer.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

impl ApplicationHandler for PrismApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let title = match self.ui_state {
            UiState::Launcher | UiState::Connecting => "PRISM",
            _ => "PRISM Client",
        };

        let size = match self.ui_state {
            UiState::Launcher | UiState::Connecting => winit::dpi::LogicalSize::new(960.0, 640.0),
            _ => winit::dpi::LogicalSize::new(1920.0, 1080.0),
        };

        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(winit::dpi::LogicalSize::new(720.0, 480.0));

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        // Initialize renderer (blocking — we're in the event loop setup)
        let renderer = pollster::block_on(PrismRenderer::new(window.clone()))
            .expect("Failed to create renderer");

        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            // Handle keyboard/mouse input in future tasks
            _ => {}
        }

        // Request continuous redraws
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
