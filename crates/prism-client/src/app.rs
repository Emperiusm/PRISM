// SPDX-License-Identifier: AGPL-3.0-or-later
//! Main application — winit event loop, wgpu renderer, UI state machine.

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::config::ClientConfig;
use crate::config::servers::ServerStore;
use crate::input::InputCoalescer;
use crate::input::double_tap::DoubleTapDetector;
use crate::renderer::PrismRenderer;
use crate::renderer::ui_renderer::UiRenderer;
use crate::session_bridge::SessionBridge;
use crate::ui::UiState;
use crate::ui::launcher::card_grid::CardGrid;
use crate::ui::launcher::quick_connect::QuickConnect;
use crate::ui::widgets::{
    MouseButton as UiMouseButton, PaintContext, Rect, TextRun, UiEvent, Widget,
};

/// Top-level PRISM application — owns the winit window, wgpu renderer, and UI state.
#[allow(dead_code)]
pub struct PrismApp {
    config: ClientConfig,
    window: Option<Arc<Window>>,
    renderer: Option<PrismRenderer>,
    ui_renderer: Option<UiRenderer>,
    ui_state: UiState,
    double_tap: DoubleTapDetector,
    coalescer: InputCoalescer,
    paint_ctx: PaintContext,
    bridge: SessionBridge,
    // Launcher widgets
    quick_connect: QuickConnect,
    card_grid: CardGrid,
    server_store: Option<ServerStore>,
    // Mouse position tracking
    mouse_x: f32,
    mouse_y: f32,
}

impl PrismApp {
    pub fn new(config: ClientConfig) -> Self {
        let ui_state = UiState::initial(config.launch_mode);

        // Try to open the server store
        let server_store = ServerStore::open(&config.servers_dir).ok();

        let mut card_grid = CardGrid::new();
        if let Some(store) = &server_store {
            card_grid.set_servers(store.servers());
        }

        Self {
            config,
            window: None,
            renderer: None,
            ui_renderer: None,
            ui_state,
            double_tap: DoubleTapDetector::new(std::time::Duration::from_millis(300)),
            coalescer: InputCoalescer::new(),
            paint_ctx: PaintContext::new(),
            bridge: SessionBridge::new(),
            quick_connect: QuickConnect::new(),
            card_grid,
            server_store,
            mouse_x: 0.0,
            mouse_y: 0.0,
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
                            r: 0.08,
                            g: 0.06,
                            b: 0.16,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        }

        // Render launcher UI when in launcher state
        if self.ui_state.shows_launcher() {
            let screen_w = renderer.width() as f32;
            let screen_h = renderer.height() as f32;

            // Layout widgets
            let padding = 24.0;

            // QuickConnect bar at y=60
            self.quick_connect
                .layout(Rect::new(padding, 60.0, screen_w - padding * 2.0, 60.0));

            // CardGrid below at y=140
            self.card_grid.layout(Rect::new(
                padding,
                140.0,
                screen_w - padding * 2.0,
                screen_h - 160.0,
            ));

            // Paint into PaintContext
            self.paint_ctx.clear();

            // Title text
            self.paint_ctx.push_text_run(TextRun {
                x: padding,
                y: 18.0,
                text: "PRISM".to_string(),
                font_size: 28.0,
                color: [1.0, 1.0, 1.0, 0.9],
                monospace: false,
            });

            // Subtitle
            self.paint_ctx.push_text_run(TextRun {
                x: padding + 100.0,
                y: 28.0,
                text: "Remote Desktop".to_string(),
                font_size: 13.0,
                color: [0.6, 0.5, 0.8, 0.6],
                monospace: false,
            });

            self.quick_connect.paint(&mut self.paint_ctx);
            self.card_grid.paint(&mut self.paint_ctx);

            // Render UI
            if let Some(ui_renderer) = &mut self.ui_renderer {
                ui_renderer.render(
                    &renderer.device,
                    &renderer.queue,
                    &mut encoder,
                    &view,
                    renderer.width(),
                    renderer.height(),
                    &self.paint_ctx,
                );
            }
        }

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
            UiState::Launcher | UiState::Connecting => "PRISM — Launcher",
            _ => "PRISM — Connected",
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

        // Initialize UI renderer
        let ui_renderer =
            UiRenderer::new(&renderer.device, &renderer.queue, renderer.surface_format());

        self.window = Some(window);
        self.ui_renderer = Some(ui_renderer);
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
            // ── Input events ──────────────────────────────────────────────
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = position.x as f32;
                self.mouse_y = position.y as f32;
                if self.ui_state.shows_launcher() {
                    let event = UiEvent::MouseMove {
                        x: self.mouse_x,
                        y: self.mouse_y,
                    };
                    let _ = self.quick_connect.handle_event(&event);
                    let _ = self.card_grid.handle_event(&event);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.ui_state.shows_launcher() {
                    let ui_button = match button {
                        winit::event::MouseButton::Left => UiMouseButton::Left,
                        winit::event::MouseButton::Right => UiMouseButton::Right,
                        winit::event::MouseButton::Middle => UiMouseButton::Middle,
                        _ => return,
                    };
                    let event = match state {
                        winit::event::ElementState::Pressed => UiEvent::MouseDown {
                            x: self.mouse_x,
                            y: self.mouse_y,
                            button: ui_button,
                        },
                        winit::event::ElementState::Released => UiEvent::MouseUp {
                            x: self.mouse_x,
                            y: self.mouse_y,
                            button: ui_button,
                        },
                    };
                    let _ = self.quick_connect.handle_event(&event);
                    let _ = self.card_grid.handle_event(&event);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if self.ui_state.shows_launcher() && event.state.is_pressed() {
                    use crate::ui::widgets::KeyCode;
                    let ui_key = match event.logical_key {
                        Key::Named(NamedKey::Enter) => Some(KeyCode::Enter),
                        Key::Named(NamedKey::Escape) => Some(KeyCode::Escape),
                        Key::Named(NamedKey::Tab) => Some(KeyCode::Tab),
                        Key::Named(NamedKey::Backspace) => Some(KeyCode::Backspace),
                        Key::Named(NamedKey::Delete) => Some(KeyCode::Delete),
                        Key::Named(NamedKey::ArrowLeft) => Some(KeyCode::Left),
                        Key::Named(NamedKey::ArrowRight) => Some(KeyCode::Right),
                        Key::Named(NamedKey::ArrowUp) => Some(KeyCode::Up),
                        Key::Named(NamedKey::ArrowDown) => Some(KeyCode::Down),
                        Key::Named(NamedKey::Home) => Some(KeyCode::Home),
                        Key::Named(NamedKey::End) => Some(KeyCode::End),
                        _ => None,
                    };
                    if let Some(key) = ui_key {
                        let ev = UiEvent::KeyDown { key };
                        let _ = self.quick_connect.handle_event(&ev);
                        let _ = self.card_grid.handle_event(&ev);
                    }

                    // Text input from character events
                    if let Key::Character(ref ch) = event.logical_key {
                        for c in ch.chars() {
                            if !c.is_control() {
                                let ev = UiEvent::TextInput { ch: c };
                                let _ = self.quick_connect.handle_event(&ev);
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        // Request continuous redraws
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
