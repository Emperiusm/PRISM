// SPDX-License-Identifier: AGPL-3.0-or-later
//! Main application — winit event loop, wgpu renderer, UI state machine.

use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::config::ClientConfig;
use crate::config::client_config_prefs::UserPrefs;
use crate::config::profiles::{ProfileConfig, ProfileStore};
use crate::config::servers::{SavedServer, ServerStore};
use crate::input::InputCoalescer;
use crate::input::double_tap::DoubleTapDetector;
use crate::renderer::PrismRenderer;
use crate::renderer::blur_pipeline::BlurPipeline;
use crate::renderer::stream_texture::StreamTexture;
use crate::renderer::ui_renderer::UiRenderer;
use crate::session_bridge::{ControlCommand, SessionBridge};
use crate::ui::launcher::card_grid::CardGrid;
use crate::ui::launcher::nav::LauncherNav;
use crate::ui::launcher::profiles::ProfilesPanel;
use crate::ui::launcher::quick_connect::QuickConnect;
use crate::ui::launcher::server_form::ServerForm;
use crate::ui::launcher::settings::SettingsPanel;
use crate::ui::launcher::shell::LauncherShell;
use crate::ui::launcher::{ActiveModal, FormMode, LauncherTab};
use crate::ui::overlay::capsule::OverlayCapsule;
use crate::ui::widgets::{
    EventResponse, MouseButton as UiMouseButton, PaintContext, Rect, UiAction, UiEvent, Widget,
};
use crate::ui::{UiState, theme};

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
    launcher_shell: LauncherShell,
    server_store: Option<ServerStore>,
    profile_store: Option<Arc<Mutex<ProfileStore>>>,
    pending_connect_profile: Option<ProfileConfig>,
    // Overlay widgets
    overlay_capsule: OverlayCapsule,
    // Connection state
    tokio_runtime: Option<tokio::runtime::Runtime>,
    stream_texture: Option<StreamTexture>,
    /// Channel to receive a connected SessionBridge from the background connect task.
    connect_result_rx: Option<std::sync::mpsc::Receiver<ConnectTaskResult>>,
    /// Stream bind group for rendering the stream fullscreen quad.
    stream_bind_group: Option<wgpu::BindGroup>,
    scene_target: Option<SceneTarget>,
    // Mouse position tracking
    mouse_x: f32,
    mouse_y: f32,
}

/// Result sent from the background connection task back to the main thread.
enum ConnectTaskResult {
    Connected { bridge: SessionBridge },
    Failed { error: String },
}

struct SceneTarget {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    blur_pipeline: BlurPipeline,
    blur_bind_group: wgpu::BindGroup,
}

impl PrismApp {
    pub fn new(config: ClientConfig) -> Self {
        let ui_state = UiState::initial(config.launch_mode);
        let identity_path = config.identity_path.display().to_string();

        // Try to open the server store
        let server_store = ServerStore::open(&config.servers_dir).ok();

        let mut card_grid = CardGrid::new();
        if let Some(store) = &server_store {
            card_grid.set_servers(store.servers());
        }
        let profile_store = ProfileStore::open(&config.servers_dir)
            .ok()
            .map(|store| Arc::new(Mutex::new(store)));
        let mut profiles_panel = ProfilesPanel::new();
        if let Some(store) = &profile_store {
            profiles_panel.set_profile_store(store.clone());
        }
        let mut settings_panel =
            SettingsPanel::new(identity_path, env!("CARGO_PKG_VERSION").to_string());
        if let Some(store) = &profile_store
            && let Ok(guard) = store.lock()
        {
            let names = guard
                .list()
                .iter()
                .map(|profile| profile.name.clone())
                .collect::<Vec<_>>();
            settings_panel.set_profile_names(names);
        }
        let user_prefs = Arc::new(Mutex::new(UserPrefs::load(&config.servers_dir)));
        settings_panel.set_user_prefs(user_prefs, config.servers_dir.clone());
        let launcher_shell = LauncherShell::new(
            LauncherNav::new(),
            QuickConnect::new(),
            card_grid,
            profiles_panel,
            settings_panel,
            ServerForm::new(),
        );

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
            launcher_shell,
            server_store,
            profile_store,
            pending_connect_profile: None,
            overlay_capsule: OverlayCapsule::new(),
            tokio_runtime: None,
            stream_texture: None,
            connect_result_rx: None,
            stream_bind_group: None,
            scene_target: None,
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

    fn make_sampled_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        label: &'static str,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(label),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        })
    }

    fn ensure_scene_target(
        scene_target: &mut Option<SceneTarget>,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        if scene_target.is_some() {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = Self::make_sampled_bind_group(device, layout, "scene-bg", &view);
        let blur_pipeline = BlurPipeline::new(device, width, height, format);
        let blur_bind_group = blur_pipeline.make_input_bind_group(device, &view);

        *scene_target = Some(SceneTarget {
            texture,
            view,
            bind_group,
            blur_pipeline,
            blur_bind_group,
        });
    }

    fn resolve_profile_for_connection(
        &self,
        raw_address: &str,
        normalized_address: &str,
    ) -> Option<ProfileConfig> {
        let preferred_profile = self.server_store.as_ref().and_then(|store| {
            store
                .servers()
                .iter()
                .find(|server| {
                    server.address == raw_address || server.address == normalized_address
                })
                .map(|server| server.default_profile.clone())
        });
        let profile_name = preferred_profile.unwrap_or_else(|| "Balanced".to_string());

        let store = self.profile_store.as_ref()?;
        let guard = store.lock().ok()?;
        guard
            .get_by_name(&profile_name)
            .cloned()
            .or_else(|| guard.get_by_name("Balanced").cloned())
            .or_else(|| guard.list().first().cloned())
    }

    fn encoder_preset_label(profile: &ProfileConfig) -> String {
        match profile.encoder_preset {
            prism_session::EncoderPreset::UltraLowLatency => "UltraLowLatency".to_string(),
            prism_session::EncoderPreset::Balanced => "Balanced".to_string(),
            prism_session::EncoderPreset::Quality => "Quality".to_string(),
        }
    }

    fn send_profile_defaults(&self, profile: &ProfileConfig) {
        self.bridge
            .send_control(ControlCommand::SwitchProfile(profile.name.clone()));
        self.bridge.send_control(ControlCommand::UpdateQuality {
            encoder_preset: Some(Self::encoder_preset_label(profile)),
            max_fps: Some(profile.max_fps),
            lossless_text: None,
            region_detection: None,
        });
        self.bridge
            .send_control(ControlCommand::SetBandwidthLimit(profile.bitrate_bps));
    }

    fn ensure_server_store(&mut self) -> Option<&mut ServerStore> {
        if self.server_store.is_none() {
            self.server_store = ServerStore::open(&self.config.servers_dir).ok();
        }
        self.server_store.as_mut()
    }

    fn refresh_saved_servers(&mut self) {
        if let Some(store) = &self.server_store {
            self.launcher_shell.set_servers(store.servers());
        } else {
            self.launcher_shell.set_servers(&[]);
        }
    }

    fn save_server_from_modal(&mut self) {
        let Some((display_name, address, noise_key, default_profile)) =
            self.launcher_shell.server_form_data()
        else {
            return;
        };
        let display_name = display_name.trim();
        let address = address.trim();
        if display_name.is_empty() || address.is_empty() {
            tracing::warn!("server form requires non-empty name and address");
            return;
        }

        let editing_id = self.launcher_shell.server_form_editing_id();
        let mut save_error: Option<std::io::Error> = None;
        if let Some(store) = self.ensure_server_store() {
            let result = if let Some(server_id) = editing_id {
                store.update(server_id, |server| {
                    server.display_name = display_name.to_string();
                    server.address = address.to_string();
                    server.noise_public_key = noise_key.clone();
                    server.default_profile = default_profile.clone();
                })
            } else {
                let mut server = SavedServer::new(display_name, address);
                server.noise_public_key = noise_key;
                server.default_profile = default_profile;
                store.add(server)
            };
            if let Err(err) = result {
                save_error = Some(err);
            }
        }

        if let Some(err) = save_error {
            tracing::error!(%err, "failed to persist saved server");
            return;
        }

        self.refresh_saved_servers();
        self.launcher_shell.dismiss_modal();
    }

    /// Initiate an async connection to the given server address on a background
    /// tokio runtime. The result is communicated back via `connect_result_rx`.
    fn start_connection(&mut self, address: &str) {
        // Default to port 7000 if the user didn't specify one
        let addr_str = if address.contains(':') {
            address.to_string()
        } else {
            format!("{address}:7000")
        };
        self.pending_connect_profile = self.resolve_profile_for_connection(address, &addr_str);
        let addr: std::net::SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(e) => {
                tracing::error!(%e, "invalid server address: {addr_str}");
                return;
            }
        };

        self.ui_state = UiState::Connecting;

        // Create tokio runtime if not yet created
        if self.tokio_runtime.is_none() {
            match tokio::runtime::Runtime::new() {
                Ok(rt) => self.tokio_runtime = Some(rt),
                Err(e) => {
                    tracing::error!(%e, "failed to create tokio runtime");
                    self.ui_state = UiState::Launcher;
                    return;
                }
            }
        }

        let (result_tx, result_rx) = std::sync::mpsc::channel();
        self.connect_result_rx = Some(result_rx);

        let noise_key = self.config.noise_key;
        let identity_path = self.config.identity_path.clone();

        let rt = self.tokio_runtime.as_ref().unwrap();
        rt.spawn(async move {
            match Self::async_connect(addr, noise_key, identity_path).await {
                Ok(bridge) => {
                    let _ = result_tx.send(ConnectTaskResult::Connected { bridge });
                }
                Err(e) => {
                    let _ = result_tx.send(ConnectTaskResult::Failed {
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    /// The async half of connection setup. Runs on the tokio runtime.
    async fn async_connect(
        addr: std::net::SocketAddr,
        noise_key: Option<[u8; 32]>,
        identity_path: std::path::PathBuf,
    ) -> Result<SessionBridge, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(%addr, "connecting to server");

        let connector = crate::ClientConnector::new(crate::TlsMode::InsecureTrustAll)?;
        let connection = connector.connect(addr, "localhost").await?;
        tracing::info!(%addr, "QUIC connected");

        // ── Load or generate client identity ─────────────────────────────
        if let Some(parent) = identity_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let identity = prism_security::identity::LocalIdentity::load_or_generate(
            &identity_path,
            "PRISM Client",
        )?;

        // ── Optional Noise IK handshake ──────────────────────────────────
        if let Some(ref server_pubkey) = noise_key {
            let (mut send, mut recv) = connection.open_bi().await?;
            let mut hs = prism_security::handshake::ClientHandshake::new(&identity, server_pubkey)?;
            let init_msg = hs.initiate()?;
            send.write_all(&init_msg).await?;
            let _ = send.finish();
            let server_response = recv.read_to_end(4096).await?;
            hs.process_response(&server_response)?;
            let _result = hs.finalize()?;
            tracing::info!("Noise IK handshake complete");
        }

        // ── Capability negotiation ───────────────────────────────────────
        {
            use prism_protocol::channel::{CHANNEL_CONTROL, CHANNEL_DISPLAY, CHANNEL_INPUT};
            use prism_session::{ClientCapabilities, ClientChannelCap, ClientPerformance};

            let (mut cap_send, mut cap_recv) = connection.open_bi().await?;

            let client_caps = ClientCapabilities {
                channels: vec![
                    ClientChannelCap {
                        channel_id: CHANNEL_DISPLAY,
                        max_version: 1,
                    },
                    ClientChannelCap {
                        channel_id: CHANNEL_INPUT,
                        max_version: 1,
                    },
                    ClientChannelCap {
                        channel_id: CHANNEL_CONTROL,
                        max_version: 1,
                    },
                ],
                performance: ClientPerformance {
                    supported_codecs: vec!["h264".into(), "h265".into()],
                },
            };

            let json = serde_json::to_vec(&client_caps)?;
            cap_send
                .write_all(&(json.len() as u32).to_le_bytes())
                .await?;
            cap_send.write_all(&json).await?;
            let _ = cap_send.finish();

            let mut len_buf = [0u8; 4];
            cap_recv.read_exact(&mut len_buf).await?;
            let resp_len = u32::from_le_bytes(len_buf) as usize;
            let mut resp_data = vec![0u8; resp_len];
            cap_recv.read_exact(&mut resp_data).await?;
            tracing::info!(bytes = resp_len, "capability negotiation complete");
        }

        // ── Create SessionBridge channels ────────────────────────────────
        let (bridge, network) = SessionBridge::create_connected();
        let crate::session_bridge::NetworkSide {
            frame_tx,
            mut control_rx,
            input_rx,
            ..
        } = network;

        // ── Spawn heartbeat ──────────────────────────────────────────────
        let hb_conn = connection.clone();
        tokio::spawn(async move {
            let ver_chan: u16 = 0x006u16;
            let mut header_bytes = [0u8; 16];
            header_bytes[0..2].copy_from_slice(&ver_chan.to_le_bytes());
            header_bytes[2] = 0x01; // HEARTBEAT
            let packet = bytes::Bytes::copy_from_slice(&header_bytes);

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                if hb_conn.send_datagram(packet.clone()).is_err() {
                    break;
                }
            }
        });

        // ── Spawn input forwarder ────────────────────────────────────────
        // Spawn control forwarder (UI control commands -> control datagrams).
        let conn_control = connection.clone();
        tokio::spawn(async move {
            use bytes::BytesMut;
            use prism_protocol::channel::CHANNEL_CONTROL;
            use prism_protocol::header::{HEADER_SIZE, PrismHeader};
            use prism_session::control_msg::{
                MONITOR_LAYOUT, PROFILE_SWITCH, ProfileSwitchPayload, QUALITY_UPDATE,
                QualityUpdatePayload, SESSION_INFO,
            };

            let mut sequence = 1_u32;
            while let Some(cmd) = control_rx.recv().await {
                let (msg_type, payload) = match cmd {
                    ControlCommand::SwitchProfile(profile_name) => {
                        let payload = ProfileSwitchPayload {
                            profile_name,
                            max_fps: 60,
                            encoder_preset: "Balanced".to_string(),
                            prefer_lossless_text: false,
                            region_detection: false,
                        };
                        let Ok(bytes) = serde_json::to_vec(&payload) else {
                            continue;
                        };
                        (PROFILE_SWITCH, bytes)
                    }
                    ControlCommand::UpdateQuality {
                        encoder_preset,
                        max_fps,
                        lossless_text,
                        region_detection,
                    } => {
                        let payload = QualityUpdatePayload {
                            encoder_preset,
                            max_fps,
                            bitrate_bps: None,
                            lossless_text,
                            region_detection,
                        };
                        let Ok(bytes) = serde_json::to_vec(&payload) else {
                            continue;
                        };
                        (QUALITY_UPDATE, bytes)
                    }
                    ControlCommand::SetBandwidthLimit(bps) => {
                        let payload = QualityUpdatePayload {
                            encoder_preset: None,
                            max_fps: None,
                            bitrate_bps: Some(bps),
                            lossless_text: None,
                            region_detection: None,
                        };
                        let Ok(bytes) = serde_json::to_vec(&payload) else {
                            continue;
                        };
                        (QUALITY_UPDATE, bytes)
                    }
                    ControlCommand::SelectMonitor(index) => {
                        let Ok(bytes) =
                            serde_json::to_vec(&serde_json::json!({ "monitor_index": index }))
                        else {
                            continue;
                        };
                        (MONITOR_LAYOUT, bytes)
                    }
                    ControlCommand::RequestServerInfo => (SESSION_INFO, Vec::new()),
                    ControlCommand::Disconnect => break,
                };

                let Ok(header) = PrismHeader::new(
                    CHANNEL_CONTROL,
                    msg_type,
                    0,
                    sequence,
                    0,
                    payload.len() as u32,
                ) else {
                    continue;
                };
                sequence = sequence.wrapping_add(1);

                let mut packet = BytesMut::with_capacity(HEADER_SIZE + payload.len());
                header.encode(&mut packet);
                packet.extend_from_slice(&payload);
                if conn_control.send_datagram(packet.freeze()).is_err() {
                    break;
                }
            }
        });

        let conn_input = connection.clone();
        tokio::spawn(async move {
            loop {
                let mut sent_any = false;
                while let Ok(dgram) = input_rx.try_recv() {
                    if conn_input.send_datagram(dgram).is_err() {
                        return;
                    }
                    sent_any = true;
                }
                if !sent_any {
                    tokio::time::sleep(std::time::Duration::from_millis(4)).await;
                }
            }
        });

        // ── Spawn frame receiver ─────────────────────────────────────────
        let conn_recv = connection.clone();
        let conn_idr = connection.clone();
        tokio::spawn(async move {
            let mut decoder = match openh264::decoder::Decoder::new() {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(%e, "failed to create H.264 decoder");
                    return;
                }
            };

            let mut gap_detector = prism_display::protocol::FrameGapDetector::with_cooldown(
                std::time::Duration::from_secs(2),
            );

            tracing::info!("waiting for persistent frame stream from server");

            let mut recv = match conn_recv.accept_uni().await {
                Ok(r) => r,
                Err(e) => {
                    tracing::info!(%e, "failed to accept frame stream");
                    return;
                }
            };

            tracing::info!("frame stream accepted");

            loop {
                // Read 4-byte length prefix
                let mut len_buf = [0u8; 4];
                if let Err(e) = recv.read_exact(&mut len_buf).await {
                    tracing::info!(%e, "frame stream closed");
                    break;
                }
                let inner_len = u32::from_le_bytes(len_buf) as usize;

                if !(16..=4 * 1024 * 1024 + 16).contains(&inner_len) {
                    tracing::error!(inner_len, "invalid frame length prefix");
                    break;
                }

                let mut header = [0u8; 16];
                if let Err(e) = recv.read_exact(&mut header).await {
                    tracing::error!(%e, "header read error");
                    break;
                }

                let width =
                    u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
                let height =
                    u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
                let seq = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
                let h264_len =
                    u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;

                if width == 0 || height == 0 || width > 4096 || height > 4096 {
                    tracing::error!(width, height, "invalid frame dimensions");
                    break;
                }
                if h264_len == 0 || h264_len > 4 * 1024 * 1024 {
                    tracing::error!(h264_len, "invalid h264_len");
                    break;
                }
                if inner_len != 16 + h264_len {
                    tracing::error!(inner_len, h264_len, "length prefix mismatch");
                    break;
                }

                let mut h264_data = vec![0u8; h264_len];
                if let Err(e) = recv.read_exact(&mut h264_data).await {
                    tracing::error!(%e, "h264 read error");
                    break;
                }

                // Frame gap detection — request IDR if needed
                gap_detector.receive_seq(seq);
                if gap_detector.should_request_idr() {
                    use prism_display::protocol::MSG_IDR_REQUEST;
                    use prism_protocol::channel::CHANNEL_DISPLAY;
                    let ver_chan: u16 = CHANNEL_DISPLAY;
                    let mut idr_bytes = [0u8; 16];
                    idr_bytes[0..2].copy_from_slice(&ver_chan.to_le_bytes());
                    idr_bytes[2] = MSG_IDR_REQUEST;
                    let idr_dgram = bytes::Bytes::copy_from_slice(&idr_bytes);
                    let _ = conn_idr.send_datagram(idr_dgram);
                }

                let yuv_frame = match decoder.decode(&h264_data) {
                    Ok(Some(yuv)) => yuv,
                    Ok(None) => continue,
                    Err(e) => {
                        tracing::error!(%e, "decode error");
                        continue;
                    }
                };

                use openh264::formats::YUVSource;
                let (dec_w, dec_h) = yuv_frame.dimensions();
                let (y_stride, u_stride, v_stride) = yuv_frame.strides();

                // Extract packed plane data (remove stride padding)
                let y_data = extract_plane(yuv_frame.y(), dec_w, dec_h, y_stride);
                let u_data = extract_plane(yuv_frame.u(), dec_w / 2, dec_h / 2, u_stride);
                let v_data = extract_plane(yuv_frame.v(), dec_w / 2, dec_h / 2, v_stride);

                let frame = crate::session_bridge::Frame {
                    width: dec_w,
                    height: dec_h,
                    y_data,
                    u_data,
                    v_data,
                    seq,
                };

                // Non-blocking send; drop on backpressure
                let _ = frame_tx.try_send(frame);
            }
        });

        // ── Spawn datagram reader (probe echo) ───────────────────────────
        let dgram_conn = connection.clone();
        tokio::spawn(async move {
            use bytes::BytesMut;
            use prism_protocol::channel::CHANNEL_CONTROL;
            use prism_protocol::header::{HEADER_SIZE, PrismHeader};
            use prism_session::control_msg::{PROBE_REQUEST, PROBE_RESPONSE};

            while let Ok(data) = dgram_conn.read_datagram().await {
                if data.len() >= HEADER_SIZE
                    && let Ok(header) = PrismHeader::decode_from_slice(&data)
                    && header.channel_id == CHANNEL_CONTROL
                    && header.msg_type == PROBE_REQUEST
                {
                    let mut response = BytesMut::from(&data[..]);
                    response[2] = PROBE_RESPONSE;
                    let _ = dgram_conn.send_datagram(response.freeze());
                }
            }
        });

        Ok(bridge)
    }

    fn route_launcher_event(&mut self, event: &UiEvent) -> EventResponse {
        if let Some(renderer) = self.renderer.as_ref() {
            self.launcher_shell.set_ui_state(self.ui_state);
            self.launcher_shell.layout(Rect::new(
                0.0,
                0.0,
                renderer.width() as f32,
                renderer.height() as f32,
            ));
        }
        self.launcher_shell.handle_event(event)
    }

    /// Handle a UiAction from widget events.
    fn handle_action(&mut self, action: UiAction) {
        match action {
            UiAction::Connect { address, .. } => {
                self.launcher_shell.set_tab(LauncherTab::Home);
                self.start_connection(&address);
            }
            UiAction::SwitchServer { address } => {
                self.launcher_shell.set_tab(LauncherTab::Home);
                self.start_connection(&address);
            }
            UiAction::OpenLauncherTab(tab) => {
                self.launcher_shell.set_tab(tab);
            }
            UiAction::Disconnect => {
                self.bridge = SessionBridge::new();
                self.stream_texture = None;
                self.stream_bind_group = None;
                self.ui_state = UiState::Launcher;
                self.launcher_shell.set_tab(LauncherTab::Home);
                self.pending_connect_profile = None;
                self.overlay_capsule.hide();
            }
            UiAction::AddServer => {
                self.launcher_shell.show_modal(ActiveModal::ServerForm {
                    mode: FormMode::Add,
                });
            }
            UiAction::SaveServer => {
                self.save_server_from_modal();
            }
            UiAction::CancelModal => {
                self.launcher_shell.dismiss_modal();
            }
            UiAction::EditServer(server_id) => {
                let server = self
                    .server_store
                    .as_ref()
                    .and_then(|store| store.get(server_id))
                    .cloned();
                if let Some(server) = server {
                    self.launcher_shell.set_server_form_editing(&server);
                    self.launcher_shell.show_modal(ActiveModal::ServerForm {
                        mode: FormMode::Edit { server_id },
                    });
                }
            }
            UiAction::DeleteServer(server_id) => {
                let server = self
                    .server_store
                    .as_ref()
                    .and_then(|store| store.get(server_id))
                    .cloned();
                if let Some(server) = server {
                    self.launcher_shell.show_modal(ActiveModal::ConfirmDelete {
                        server_id,
                        name: server.display_name,
                    });
                }
            }
            UiAction::ConfirmDeleteServer(server_id) => {
                let mut delete_error: Option<std::io::Error> = None;
                if let Some(store) = self.ensure_server_store()
                    && let Err(err) = store.delete(server_id)
                {
                    delete_error = Some(err);
                }

                if let Some(err) = delete_error {
                    tracing::error!(%err, "failed to delete saved server");
                } else {
                    self.refresh_saved_servers();
                }
                self.launcher_shell.dismiss_modal();
            }
            UiAction::CloseOverlay => {
                self.ui_state = UiState::Stream;
                self.overlay_capsule.hide();
            }
            UiAction::OpenPanel(_) => {
                // Overlay capsule handles panel expansion internally.
            }
            UiAction::ClosePanel(_) => {
                // Overlay capsule handles panel collapse internally.
            }
            UiAction::TogglePinStatsBar => {
                // Pinning is currently local to the overlay widget.
            }
            UiAction::SwitchProfile(profile_name) => {
                self.bridge
                    .send_control(ControlCommand::SwitchProfile(profile_name));
            }
            UiAction::UpdateQuality {
                preset,
                max_fps,
                lossless_text,
                region_detection,
            } => {
                self.bridge.send_control(ControlCommand::UpdateQuality {
                    encoder_preset: preset,
                    max_fps,
                    lossless_text,
                    region_detection,
                });
            }
            UiAction::SetBandwidthLimit(bps) => {
                self.bridge
                    .send_control(ControlCommand::SetBandwidthLimit(bps));
            }
            UiAction::SelectMonitor(index) => {
                self.bridge
                    .send_control(ControlCommand::SelectMonitor(index));
            }
        }
    }

    /// Poll for connection completion from the background task.
    fn poll_connection(&mut self) {
        let rx = match &self.connect_result_rx {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(ConnectTaskResult::Connected { bridge }) => {
                tracing::info!("connection established");
                self.bridge = bridge;
                self.ui_state = UiState::Stream;
                if let Some(profile) = self.pending_connect_profile.take() {
                    self.send_profile_defaults(&profile);
                }
                self.connect_result_rx = None;
            }
            Ok(ConnectTaskResult::Failed { error }) => {
                tracing::error!(%error, "connection failed");
                self.ui_state = UiState::Launcher;
                self.pending_connect_profile = None;
                self.connect_result_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                // Still connecting
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                tracing::error!("connection task channel disconnected");
                self.ui_state = UiState::Launcher;
                self.pending_connect_profile = None;
                self.connect_result_rx = None;
            }
        }
    }

    fn render(&mut self) {
        self.poll_connection();

        let output = {
            let renderer = match self.renderer.as_ref() {
                Some(r) => r,
                None => return,
            };
            match renderer.surface.get_current_texture() {
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
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .renderer
            .as_ref()
            .expect("renderer exists")
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        {
            let renderer = self.renderer.as_ref().expect("renderer exists");
            Self::ensure_scene_target(
                &mut self.scene_target,
                &renderer.device,
                &renderer.stream_bind_group_layout,
                renderer.width(),
                renderer.height(),
                renderer.surface_format(),
            );
        }

        if self.scene_target.is_none() {
            return;
        }

        // ── Stream rendering (Stream and Overlay states) ─────────────────
        if self.ui_state.shows_stream() {
            // Drain all pending frames; use the latest
            let mut latest_frame = None;
            while let Some(frame) = self.bridge.try_recv_frame() {
                latest_frame = Some(frame);
            }

            if let Some(frame) = latest_frame {
                // Create or resize stream texture if needed
                let needs_recreate = match &self.stream_texture {
                    Some(st) => st.width != frame.width as u32 || st.height != frame.height as u32,
                    None => true,
                };

                if needs_recreate {
                    let renderer = self.renderer.as_ref().expect("renderer exists");
                    let st = StreamTexture::new(
                        &renderer.device,
                        frame.width as u32,
                        frame.height as u32,
                    );
                    let bind_group = Self::make_sampled_bind_group(
                        &renderer.device,
                        &renderer.stream_bind_group_layout,
                        "stream-bg",
                        st.output_view(),
                    );
                    self.stream_bind_group = Some(bind_group);
                    self.stream_texture = Some(st);
                }

                if let Some(st) = &mut self.stream_texture {
                    let renderer = self.renderer.as_ref().expect("renderer exists");
                    st.upload_yuv(&renderer.queue, &frame.y_data, &frame.u_data, &frame.v_data);
                }
            }

            if let Some(st) = &mut self.stream_texture {
                st.convert(&mut encoder);
            }
        }

        {
            let scene = self.scene_target.as_ref().expect("scene target");
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: theme::BACKDROP[0],
                            g: theme::BACKDROP[1],
                            b: theme::BACKDROP[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        }

        if self.ui_state.shows_stream()
            && let Some(bg) = &self.stream_bind_group
        {
            let scene = self.scene_target.as_ref().expect("scene target");
            let renderer = self.renderer.as_ref().expect("renderer exists");
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Stream Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &scene.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&renderer.stream_pipeline);
            pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
            pass.set_bind_group(1, bg, &[]);
            pass.draw(0..3, 0..1);
        }

        {
            let scene = self.scene_target.as_ref().expect("scene target");
            scene
                .blur_pipeline
                .run(&mut encoder, &scene.blur_bind_group);
        }

        {
            let scene = self.scene_target.as_ref().expect("scene target");
            let renderer = self.renderer.as_ref().expect("renderer exists");
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene Composite Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: theme::BACKDROP[0],
                            g: theme::BACKDROP[1],
                            b: theme::BACKDROP[2],
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&renderer.stream_pipeline);
            pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
            pass.set_bind_group(1, &scene.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // ── Overlay rendering ────────────────────────────────────────────
        if self.ui_state.shows_overlay() {
            let renderer = self.renderer.as_ref().expect("renderer exists");
            let screen_w = renderer.width() as f32;
            let screen_h = renderer.height() as f32;

            // Update stats from bridge
            if let Some(stats) = self.bridge.current_stats() {
                self.overlay_capsule.update_stats(stats);
            }

            self.paint_ctx.clear();
            self.overlay_capsule.show();
            self.overlay_capsule
                .layout(Rect::new(0.0, 0.0, screen_w, screen_h));
            self.overlay_capsule.paint(&mut self.paint_ctx);

            // Render overlay UI on top of stream
            if let Some(ui_renderer) = &mut self.ui_renderer {
                let backdrop_view = self
                    .scene_target
                    .as_ref()
                    .expect("scene target")
                    .blur_pipeline
                    .output_view();
                ui_renderer.render(
                    &renderer.device,
                    &renderer.queue,
                    &mut encoder,
                    &view,
                    backdrop_view,
                    renderer.width(),
                    renderer.height(),
                    &self.paint_ctx,
                );
            }
        }

        // ── Launcher UI ──────────────────────────────────────────────────
        if self.ui_state.shows_launcher() {
            let (screen_w, screen_h) = {
                let renderer = self.renderer.as_ref().expect("renderer exists");
                (renderer.width() as f32, renderer.height() as f32)
            };
            self.paint_ctx.clear();
            self.launcher_shell.set_ui_state(self.ui_state);
            self.launcher_shell
                .layout(Rect::new(0.0, 0.0, screen_w, screen_h));
            self.launcher_shell.paint(&mut self.paint_ctx);

            // Render UI
            if let Some(ui_renderer) = &mut self.ui_renderer {
                let renderer = self.renderer.as_ref().expect("renderer exists");
                let backdrop_view = self
                    .scene_target
                    .as_ref()
                    .expect("scene target")
                    .blur_pipeline
                    .output_view();
                ui_renderer.render(
                    &renderer.device,
                    &renderer.queue,
                    &mut encoder,
                    &view,
                    backdrop_view,
                    renderer.width(),
                    renderer.height(),
                    &self.paint_ctx,
                );
            }
        }

        self.renderer
            .as_ref()
            .expect("renderer exists")
            .queue
            .submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

/// Extract a plane from a YUV frame, removing stride padding.
fn extract_plane(plane: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    if stride == width {
        return plane[..width * height].to_vec();
    }
    let mut out = Vec::with_capacity(width * height);
    for row in 0..height {
        let start = row * stride;
        let end = start + width;
        if end <= plane.len() {
            out.extend_from_slice(&plane[start..end]);
        } else {
            // Pad with zeros if data is short
            let available = plane.len().saturating_sub(start);
            if available > 0 {
                out.extend_from_slice(&plane[start..start + available.min(width)]);
            }
            out.resize(out.len() + width - available.min(width), 0);
        }
    }
    out
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
                    self.scene_target = None;
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
                    let _ = self.route_launcher_event(&event);
                }
                if self.ui_state.shows_overlay() {
                    let event = UiEvent::MouseMove {
                        x: self.mouse_x,
                        y: self.mouse_y,
                    };
                    let _ = self.overlay_capsule.handle_event(&event);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let ui_button = match button {
                    winit::event::MouseButton::Left => UiMouseButton::Left,
                    winit::event::MouseButton::Right => UiMouseButton::Right,
                    winit::event::MouseButton::Middle => UiMouseButton::Middle,
                    _ => return,
                };
                let ui_event = match state {
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

                if self.ui_state.shows_launcher() {
                    match self.route_launcher_event(&ui_event) {
                        EventResponse::Action(action) => {
                            self.handle_action(action);
                            return;
                        }
                        EventResponse::Consumed => return,
                        EventResponse::Ignored => {}
                    }
                }

                if self.ui_state.shows_overlay() {
                    match self.overlay_capsule.handle_event(&ui_event) {
                        EventResponse::Action(action) => {
                            self.handle_action(action);
                            return;
                        }
                        EventResponse::Consumed => return,
                        EventResponse::Ignored => {}
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // ── Double-tap Left Ctrl detection for overlay toggle ─────
                if let Key::Named(NamedKey::Control) = event.logical_key {
                    if event.state.is_pressed() {
                        let now = Instant::now();
                        if self.double_tap.key_down(now) {
                            // Toggle overlay
                            match self.ui_state {
                                UiState::Stream => {
                                    self.ui_state = UiState::Overlay;
                                    self.overlay_capsule.show();
                                }
                                UiState::Overlay => {
                                    self.ui_state = UiState::Stream;
                                    self.overlay_capsule.hide();
                                }
                                _ => {}
                            }
                            self.double_tap.consume();
                        }
                    } else {
                        self.double_tap.key_up(Instant::now());
                    }
                } else if event.state.is_pressed() {
                    // Any other key cancels double-tap detection
                    self.double_tap.other_key_pressed();
                }

                // ── Launcher keyboard events ─────────────────────────────
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
                        if let EventResponse::Action(action) = self.route_launcher_event(&ev) {
                            self.handle_action(action);
                        }
                    }

                    // Text input from character events
                    if let Key::Character(ref ch) = event.logical_key {
                        for c in ch.chars() {
                            if !c.is_control() {
                                let ev = UiEvent::TextInput { ch: c };
                                let _ = self.route_launcher_event(&ev);
                            }
                        }
                    }
                }

                // ── Forward input to remote when in Stream state ─────────
                if self.ui_state.forwards_input() && event.state.is_pressed() {
                    // Map winit key to scancode and forward via bridge
                    if let Key::Named(named) = &event.logical_key {
                        let vk = winit_named_to_vk(named);
                        if vk != 0 {
                            let mut sender = crate::InputSender::new();
                            let evt =
                                prism_protocol::input::InputEvent::KeyDown { scancode: vk, vk };
                            let dgram = sender.build_datagram(evt);
                            self.bridge.send_input(dgram);
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

/// Map a winit NamedKey to a Windows virtual key code (best-effort).
fn winit_named_to_vk(key: &NamedKey) -> u16 {
    match key {
        NamedKey::Enter => 0x0D,
        NamedKey::Escape => 0x1B,
        NamedKey::Tab => 0x09,
        NamedKey::Backspace => 0x08,
        NamedKey::Delete => 0x2E,
        NamedKey::ArrowLeft => 0x25,
        NamedKey::ArrowRight => 0x27,
        NamedKey::ArrowUp => 0x26,
        NamedKey::ArrowDown => 0x28,
        NamedKey::Home => 0x24,
        NamedKey::End => 0x23,
        NamedKey::Space => 0x20,
        NamedKey::Control => 0x11,
        NamedKey::Shift => 0x10,
        NamedKey::Alt => 0x12,
        _ => 0,
    }
}
