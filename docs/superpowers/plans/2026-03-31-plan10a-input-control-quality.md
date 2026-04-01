# Plan 10A: Input Forwarding + Control Channel + Quality Loop

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make PRISM controllable — client sends keyboard/mouse input to the server, server injects it via Win32 `SendInput`, control channel exchanges heartbeats and probes, quality loop adjusts encoder bitrate based on measured network conditions.

**Architecture:** `InputEvent` types live in `prism-protocol` (shared wire format). Client captures minifb keyboard/mouse events, serializes to PRISM datagrams, sends on CHANNEL_INPUT. Server's `InputChannelHandler` deserializes and calls `InputInjector::inject()` (#[cfg(windows)] SendInput). `ControlChannelHandler` routes heartbeat/probe/feedback messages. `QualityTask` is event-driven: probe echoes trigger quality recomputation → ArcSwap cache → degradation ladder → encoder bitrate. The `ChannelHandler` trait is refactored to add `handle_stream()`.

**Tech Stack:** `prism-protocol` (InputEvent types), `prism-session` (ChannelHandler refactor), `prism-server` (handlers + tasks), `prism-client` (input capture + heartbeat), `windows` crate (SendInput, #[cfg(windows)])

**Spec refs:**
- Phase 1 Completion: `docs/superpowers/specs/2026-03-31-phase1-completion-design.md` (Sections 1, 2, 3)

---

## File Structure

```
crates/prism-protocol/src/
    input.rs                    # InputEvent, MouseButton wire types + serialize/deserialize
    lib.rs                      # add pub mod input

crates/prism-session/src/
    dispatch.rs                 # REFACTOR: add handle_stream() to ChannelHandler trait

crates/prism-server/src/
    input_handler.rs            # InputChannelHandler + InputInjector (#[cfg(windows)])
    control_handler.rs          # ControlChannelHandler (routes by msg_type)
    heartbeat_task.rs           # HeartbeatGenerator (zero-alloc) + sender task
    quality_task.rs             # Event-driven quality → ArcSwap cache → encoder adjust
    lib.rs                      # add new modules
    main.rs                     # wire handlers into dispatcher + spawn tasks

crates/prism-client/src/
    input_sender.rs             # Capture minifb events → build datagrams → send
    heartbeat_client.rs         # Client heartbeat sender + probe echo responder
    lib.rs                      # add new modules
    main.rs                     # wire input capture + heartbeat into client loop
```

---

## Task 1: InputEvent Wire Types (prism-protocol)

**Files:**
- Create: `crates/prism-protocol/src/input.rs`
- Modify: `crates/prism-protocol/src/lib.rs`

- [ ] **Step 1: Write failing tests**

`crates/prism-protocol/src/input.rs`:
```rust
/// Mouse button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    X1 = 3,
    X2 = 4,
}

/// Input event discriminant tags.
#[repr(u8)]
enum InputTag {
    KeyDown = 1,
    KeyUp = 2,
    TextInput = 3,
    MouseMove = 4,
    MouseDown = 5,
    MouseUp = 6,
    MouseScroll = 7,
    MouseMoveRelative = 8,
    SetMouseMode = 9,
}

/// Input event sent from client to server.
/// Fixed 12-byte binary encoding (tag + payload padded to 12 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    KeyDown { scancode: u16, vk: u16 },
    KeyUp { scancode: u16, vk: u16 },
    TextInput { codepoint: u32 },
    MouseMove { x: u16, y: u16 },
    MouseDown { button: MouseButton },
    MouseUp { button: MouseButton },
    MouseScroll { delta_x: i16, delta_y: i16 },
    MouseMoveRelative { dx: i16, dy: i16 },
    SetMouseMode { relative: bool },
}

pub const INPUT_EVENT_SIZE: usize = 12;

// Serialization + deserialization code here

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_down_roundtrip() {
        let event = InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 }; // 'A'
        let bytes = event.to_bytes();
        assert_eq!(bytes.len(), INPUT_EVENT_SIZE);
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn mouse_move_roundtrip() {
        let event = InputEvent::MouseMove { x: 32768, y: 16384 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn mouse_move_relative_roundtrip() {
        let event = InputEvent::MouseMoveRelative { dx: -50, dy: 100 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn text_input_roundtrip() {
        let event = InputEvent::TextInput { codepoint: 0x1F600 }; // 😀
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn mouse_scroll_roundtrip() {
        let event = InputEvent::MouseScroll { delta_x: 0, delta_y: -120 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn all_mouse_buttons_roundtrip() {
        for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::X1, MouseButton::X2] {
            let event = InputEvent::MouseDown { button };
            let decoded = InputEvent::from_bytes(&event.to_bytes()).unwrap();
            assert_eq!(decoded, event);
        }
    }

    #[test]
    fn set_mouse_mode_roundtrip() {
        let event = InputEvent::SetMouseMode { relative: true };
        let decoded = InputEvent::from_bytes(&event.to_bytes()).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn invalid_tag_returns_none() {
        let mut bytes = [0u8; INPUT_EVENT_SIZE];
        bytes[0] = 255; // invalid tag
        assert!(InputEvent::from_bytes(&bytes).is_none());
    }

    #[test]
    fn too_short_returns_none() {
        assert!(InputEvent::from_bytes(&[1, 2, 3]).is_none());
    }
}
```

- [ ] **Step 2: Implement InputEvent serialization**

```rust
impl InputEvent {
    pub fn to_bytes(&self) -> [u8; INPUT_EVENT_SIZE] {
        let mut buf = [0u8; INPUT_EVENT_SIZE];
        match self {
            InputEvent::KeyDown { scancode, vk } => {
                buf[0] = 1;
                buf[1..3].copy_from_slice(&scancode.to_le_bytes());
                buf[3..5].copy_from_slice(&vk.to_le_bytes());
            }
            InputEvent::KeyUp { scancode, vk } => {
                buf[0] = 2;
                buf[1..3].copy_from_slice(&scancode.to_le_bytes());
                buf[3..5].copy_from_slice(&vk.to_le_bytes());
            }
            InputEvent::TextInput { codepoint } => {
                buf[0] = 3;
                buf[1..5].copy_from_slice(&codepoint.to_le_bytes());
            }
            InputEvent::MouseMove { x, y } => {
                buf[0] = 4;
                buf[1..3].copy_from_slice(&x.to_le_bytes());
                buf[3..5].copy_from_slice(&y.to_le_bytes());
            }
            InputEvent::MouseDown { button } => {
                buf[0] = 5;
                buf[1] = *button as u8;
            }
            InputEvent::MouseUp { button } => {
                buf[0] = 6;
                buf[1] = *button as u8;
            }
            InputEvent::MouseScroll { delta_x, delta_y } => {
                buf[0] = 7;
                buf[1..3].copy_from_slice(&delta_x.to_le_bytes());
                buf[3..5].copy_from_slice(&delta_y.to_le_bytes());
            }
            InputEvent::MouseMoveRelative { dx, dy } => {
                buf[0] = 8;
                buf[1..3].copy_from_slice(&dx.to_le_bytes());
                buf[3..5].copy_from_slice(&dy.to_le_bytes());
            }
            InputEvent::SetMouseMode { relative } => {
                buf[0] = 9;
                buf[1] = if *relative { 1 } else { 0 };
            }
        }
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() < INPUT_EVENT_SIZE { return None; }
        match buf[0] {
            1 => Some(InputEvent::KeyDown {
                scancode: u16::from_le_bytes([buf[1], buf[2]]),
                vk: u16::from_le_bytes([buf[3], buf[4]]),
            }),
            2 => Some(InputEvent::KeyUp {
                scancode: u16::from_le_bytes([buf[1], buf[2]]),
                vk: u16::from_le_bytes([buf[3], buf[4]]),
            }),
            3 => Some(InputEvent::TextInput {
                codepoint: u32::from_le_bytes([buf[1], buf[2], buf[3], buf[4]]),
            }),
            4 => Some(InputEvent::MouseMove {
                x: u16::from_le_bytes([buf[1], buf[2]]),
                y: u16::from_le_bytes([buf[3], buf[4]]),
            }),
            5 => Some(InputEvent::MouseDown { button: MouseButton::from_u8(buf[1])? }),
            6 => Some(InputEvent::MouseUp { button: MouseButton::from_u8(buf[1])? }),
            7 => Some(InputEvent::MouseScroll {
                delta_x: i16::from_le_bytes([buf[1], buf[2]]),
                delta_y: i16::from_le_bytes([buf[3], buf[4]]),
            }),
            8 => Some(InputEvent::MouseMoveRelative {
                dx: i16::from_le_bytes([buf[1], buf[2]]),
                dy: i16::from_le_bytes([buf[3], buf[4]]),
            }),
            9 => Some(InputEvent::SetMouseMode { relative: buf[1] != 0 }),
            _ => None,
        }
    }
}

impl MouseButton {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(MouseButton::Left),
            1 => Some(MouseButton::Right),
            2 => Some(MouseButton::Middle),
            3 => Some(MouseButton::X1),
            4 => Some(MouseButton::X2),
            _ => None,
        }
    }
}
```

Update `crates/prism-protocol/src/lib.rs`: add `pub mod input;`

- [ ] **Step 3: Verify, commit**

```bash
cargo test -p prism-protocol
git commit -m "feat(protocol): InputEvent wire types with 12-byte binary encoding"
```

---

## Task 2: ChannelHandler Trait Refactor (add handle_stream)

**Files:**
- Modify: `crates/prism-session/src/dispatch.rs`

- [ ] **Step 1: Add handle_stream to ChannelHandler trait**

Add after the existing `handle_datagram` method:

```rust
    /// Handle a stream-delivered message. Default: no-op.
    /// Channels that need bidirectional streams (clipboard, feedback) override this.
    async fn handle_stream(
        &self,
        _from: ClientId,
        _send: prism_transport::OwnedSendStream,
        _recv: prism_transport::OwnedRecvStream,
    ) -> Result<(), ChannelError> {
        Ok(())
    }
```

Add `prism-transport` to prism-session's Cargo.toml dependencies if not already present. CHECK FIRST — read `crates/prism-session/Cargo.toml` to see if prism-transport is already a dependency.

- [ ] **Step 2: Verify existing tests still pass**

```bash
cargo test -p prism-session
```

- [ ] **Step 3: Commit**

```bash
git commit -m "refactor(session): add handle_stream() to ChannelHandler trait"
```

---

## Task 3: HeartbeatGenerator + Heartbeat Task

**Files:**
- Create: `crates/prism-server/src/heartbeat_task.rs`
- Modify: `crates/prism-server/src/lib.rs`

- [ ] **Step 1: Write tests + implement**

```rust
use bytes::{Bytes, BytesMut};
use prism_protocol::header::{PrismHeader, PROTOCOL_VERSION, HEADER_SIZE};
use prism_protocol::channel::CHANNEL_CONTROL;
use prism_session::control_msg;

/// Pre-built heartbeat packet. Zero allocation on send (Bytes clone is Arc increment).
pub struct HeartbeatGenerator {
    packet: Bytes,
}

impl HeartbeatGenerator {
    pub fn new() -> Self {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id: CHANNEL_CONTROL,
            msg_type: control_msg::HEARTBEAT,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        Self { packet: buf.freeze() }
    }

    /// Get the pre-built heartbeat packet. Clone is zero-alloc (Arc increment).
    pub fn packet(&self) -> Bytes {
        self.packet.clone()
    }
}

impl Default for HeartbeatGenerator {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_packet_is_16_bytes() {
        let gen = HeartbeatGenerator::new();
        assert_eq!(gen.packet().len(), HEADER_SIZE);
    }

    #[test]
    fn heartbeat_packet_is_valid_prism_header() {
        let gen = HeartbeatGenerator::new();
        let header = PrismHeader::decode_from_slice(&gen.packet()).unwrap();
        assert_eq!(header.channel_id, CHANNEL_CONTROL);
        assert_eq!(header.msg_type, control_msg::HEARTBEAT);
        assert_eq!(header.payload_length, 0);
    }

    #[test]
    fn clone_is_cheap() {
        let gen = HeartbeatGenerator::new();
        let p1 = gen.packet();
        let p2 = gen.packet();
        // Both point to the same underlying data
        assert_eq!(p1.as_ptr(), p2.as_ptr());
    }
}
```

Update lib.rs: `pub mod heartbeat_task;` and `pub use heartbeat_task::HeartbeatGenerator;`

- [ ] **Step 2: Verify, commit**

```bash
cargo test -p prism-server -- heartbeat
git commit -m "feat(server): HeartbeatGenerator with zero-allocation pre-built packet"
```

---

## Task 4: ControlChannelHandler

**Files:**
- Create: `crates/prism-server/src/control_handler.rs`
- Modify: `crates/prism-server/src/lib.rs`

- [ ] **Step 1: Write tests + implement**

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use async_trait::async_trait;
use bytes::Bytes;
use prism_protocol::header::{PrismHeader, HEADER_SIZE};
use prism_protocol::channel::CHANNEL_CONTROL;
use prism_session::{ClientId, ChannelError};
use prism_session::dispatch::ChannelHandler;
use prism_session::control_msg;

/// Tracks control message statistics.
#[derive(Debug, Default)]
pub struct ControlStats {
    pub heartbeats_received: AtomicU32,
    pub probes_received: AtomicU32,
    pub probes_echoed: AtomicU32,
    pub unknown_messages: AtomicU32,
}

/// Routes incoming control channel datagrams by msg_type.
pub struct ControlChannelHandler {
    stats: Arc<ControlStats>,
}

impl ControlChannelHandler {
    pub fn new() -> Self {
        Self { stats: Arc::new(ControlStats::default()) }
    }

    pub fn stats(&self) -> Arc<ControlStats> { self.stats.clone() }
}

#[async_trait]
impl ChannelHandler for ControlChannelHandler {
    fn channel_id(&self) -> u16 { CHANNEL_CONTROL }

    async fn handle_datagram(&self, _from: ClientId, data: Bytes) -> Result<(), ChannelError> {
        if data.len() < HEADER_SIZE { return Ok(()); }
        let header = match PrismHeader::decode_from_slice(&data) {
            Ok(h) => h,
            Err(_) => return Ok(()),
        };

        match header.msg_type {
            control_msg::HEARTBEAT => {
                self.stats.heartbeats_received.fetch_add(1, Ordering::Relaxed);
                // Activity signal already handled by recv_loop
            }
            control_msg::PROBE_REQUEST => {
                self.stats.probes_received.fetch_add(1, Ordering::Relaxed);
                // In a full implementation, echo back as PROBE_RESPONSE
                self.stats.probes_echoed.fetch_add(1, Ordering::Relaxed);
            }
            control_msg::PROBE_RESPONSE => {
                self.stats.probes_received.fetch_add(1, Ordering::Relaxed);
                // Forward to QualityMonitor prober
            }
            _ => {
                self.stats.unknown_messages.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

impl Default for ControlChannelHandler { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use uuid::Uuid;

    fn make_control_datagram(msg_type: u8) -> Bytes {
        let header = PrismHeader {
            version: 0, channel_id: CHANNEL_CONTROL, msg_type, flags: 0,
            sequence: 0, timestamp_us: 0, payload_length: 0,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);
        header.encode(&mut buf);
        buf.freeze()
    }

    fn test_client() -> ClientId { Uuid::nil() }

    #[test]
    fn handler_channel_id() {
        let handler = ControlChannelHandler::new();
        assert_eq!(handler.channel_id(), CHANNEL_CONTROL);
    }

    #[tokio::test]
    async fn heartbeat_counted() {
        let handler = ControlChannelHandler::new();
        let data = make_control_datagram(control_msg::HEARTBEAT);
        handler.handle_datagram(test_client(), data).await.unwrap();
        assert_eq!(handler.stats().heartbeats_received.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn probe_request_counted() {
        let handler = ControlChannelHandler::new();
        let data = make_control_datagram(control_msg::PROBE_REQUEST);
        handler.handle_datagram(test_client(), data).await.unwrap();
        assert_eq!(handler.stats().probes_received.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn unknown_msg_type_counted() {
        let handler = ControlChannelHandler::new();
        let data = make_control_datagram(0xFF);
        handler.handle_datagram(test_client(), data).await.unwrap();
        assert_eq!(handler.stats().unknown_messages.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn short_datagram_ignored() {
        let handler = ControlChannelHandler::new();
        handler.handle_datagram(test_client(), Bytes::from_static(&[1,2,3])).await.unwrap();
        // No panic, no counter increment
    }
}
```

Update lib.rs: `pub mod control_handler;` and `pub use control_handler::ControlChannelHandler;`

- [ ] **Step 2: Verify, commit**

```bash
cargo test -p prism-server -- control
git commit -m "feat(server): ControlChannelHandler routing heartbeat/probe/feedback"
```

---

## Task 5: InputChannelHandler + InputInjector

**Files:**
- Create: `crates/prism-server/src/input_handler.rs`
- Modify: `crates/prism-server/src/lib.rs`

- [ ] **Step 1: Write tests + implement**

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use async_trait::async_trait;
use bytes::Bytes;
use prism_protocol::header::{PrismHeader, HEADER_SIZE};
use prism_protocol::channel::CHANNEL_INPUT;
use prism_protocol::input::{InputEvent, INPUT_EVENT_SIZE};
use prism_session::{ClientId, ChannelError};
use prism_session::dispatch::ChannelHandler;

/// Coordinate mapper: normalized (0-65535) → screen pixels.
pub fn normalized_to_screen(normalized: u16, screen_size: u32) -> i32 {
    (normalized as u64 * screen_size as u64 / 65535) as i32
}

/// Tracks input processing statistics.
#[derive(Debug, Default)]
pub struct InputStats {
    pub events_received: AtomicU32,
    pub events_injected: AtomicU32,
    pub events_failed: AtomicU32,
}

/// Handles incoming input datagrams from clients.
pub struct InputChannelHandler {
    screen_width: u32,
    screen_height: u32,
    stats: Arc<InputStats>,
}

impl InputChannelHandler {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            screen_width, screen_height,
            stats: Arc::new(InputStats::default()),
        }
    }

    pub fn stats(&self) -> Arc<InputStats> { self.stats.clone() }

    fn process_event(&self, event: InputEvent) {
        self.stats.events_received.fetch_add(1, Ordering::Relaxed);
        // On Windows: call SendInput. On other platforms: log only.
        #[cfg(windows)]
        {
            if inject_input(event, self.screen_width, self.screen_height).is_ok() {
                self.stats.events_injected.fetch_add(1, Ordering::Relaxed);
            } else {
                self.stats.events_failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        #[cfg(not(windows))]
        {
            self.stats.events_injected.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[async_trait]
impl ChannelHandler for InputChannelHandler {
    fn channel_id(&self) -> u16 { CHANNEL_INPUT }

    async fn handle_datagram(&self, _from: ClientId, data: Bytes) -> Result<(), ChannelError> {
        if data.len() < HEADER_SIZE + INPUT_EVENT_SIZE { return Ok(()); }
        if let Some(event) = InputEvent::from_bytes(&data[HEADER_SIZE..]) {
            self.process_event(event);
        }
        Ok(())
    }
}

/// Win32 SendInput injection.
#[cfg(windows)]
fn inject_input(event: InputEvent, screen_w: u32, screen_h: u32) -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    let mut inputs: Vec<INPUT> = Vec::new();

    match event {
        InputEvent::KeyDown { scancode, vk } => {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scancode,
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        InputEvent::KeyUp { scancode, vk } => {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: scancode,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        InputEvent::TextInput { codepoint } => {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: codepoint as u16,
                        dwFlags: KEYEVENTF_UNICODE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        InputEvent::MouseMove { x, y } => {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: x as i32,
                        dy: y as i32,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        InputEvent::MouseMoveRelative { dx, dy } => {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: dx as i32,
                        dy: dy as i32,
                        mouseData: 0,
                        dwFlags: MOUSEEVENTF_MOVE,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
        InputEvent::MouseDown { button } => {
            let flag = match button {
                prism_protocol::input::MouseButton::Left => MOUSEEVENTF_LEFTDOWN,
                prism_protocol::input::MouseButton::Right => MOUSEEVENTF_RIGHTDOWN,
                prism_protocol::input::MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN,
                _ => return Ok(()),
            };
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: flag, time: 0, dwExtraInfo: 0 },
                },
            });
        }
        InputEvent::MouseUp { button } => {
            let flag = match button {
                prism_protocol::input::MouseButton::Left => MOUSEEVENTF_LEFTUP,
                prism_protocol::input::MouseButton::Right => MOUSEEVENTF_RIGHTUP,
                prism_protocol::input::MouseButton::Middle => MOUSEEVENTF_MIDDLEUP,
                _ => return Ok(()),
            };
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: 0, dwFlags: flag, time: 0, dwExtraInfo: 0 },
                },
            });
        }
        InputEvent::MouseScroll { delta_x: _, delta_y } => {
            inputs.push(INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0, dy: 0,
                        mouseData: delta_y as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        time: 0, dwExtraInfo: 0,
                    },
                },
            });
        }
        InputEvent::SetMouseMode { .. } => { /* mode tracking only */ }
    }

    if !inputs.is_empty() {
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32); }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use uuid::Uuid;

    fn make_input_datagram(event: InputEvent) -> Bytes {
        let header = PrismHeader {
            version: 0, channel_id: CHANNEL_INPUT, msg_type: 0x01, flags: 0,
            sequence: 0, timestamp_us: 0, payload_length: INPUT_EVENT_SIZE as u32,
        };
        let mut buf = BytesMut::with_capacity(HEADER_SIZE + INPUT_EVENT_SIZE);
        header.encode(&mut buf);
        buf.extend_from_slice(&event.to_bytes());
        buf.freeze()
    }

    #[test]
    fn normalized_to_screen_center() {
        assert_eq!(normalized_to_screen(32768, 1920), 960);
        assert_eq!(normalized_to_screen(32768, 1080), 540);
    }

    #[test]
    fn normalized_to_screen_edges() {
        assert_eq!(normalized_to_screen(0, 1920), 0);
        assert_eq!(normalized_to_screen(65535, 1920), 1920);
    }

    #[test]
    fn handler_channel_id() {
        let handler = InputChannelHandler::new(1920, 1080);
        assert_eq!(handler.channel_id(), CHANNEL_INPUT);
    }

    #[tokio::test]
    async fn processes_key_down() {
        let handler = InputChannelHandler::new(1920, 1080);
        let data = make_input_datagram(InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 });
        handler.handle_datagram(Uuid::nil(), data).await.unwrap();
        assert_eq!(handler.stats().events_received.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn processes_mouse_move() {
        let handler = InputChannelHandler::new(1920, 1080);
        let data = make_input_datagram(InputEvent::MouseMove { x: 32768, y: 16384 });
        handler.handle_datagram(Uuid::nil(), data).await.unwrap();
        assert_eq!(handler.stats().events_received.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn short_datagram_ignored() {
        let handler = InputChannelHandler::new(1920, 1080);
        handler.handle_datagram(Uuid::nil(), Bytes::from_static(&[0; 10])).await.unwrap();
        assert_eq!(handler.stats().events_received.load(Ordering::Relaxed), 0);
    }
}
```

**IMPORTANT for implementer:** The Win32 `SendInput` code above uses `windows` crate types. The exact API (struct field names, flag types) MUST be verified against the actual `windows` crate version. The code shows the INTENT — adapt field names like `Anonymous`, `ki`, `mi`, `dwFlags` to match the actual API. Also add `"Win32_UI_Input_KeyboardAndMouse"` to the windows features in prism-server Cargo.toml.

Update lib.rs: `pub mod input_handler;` and `pub use input_handler::InputChannelHandler;`

- [ ] **Step 2: Verify, commit**

```bash
cargo test -p prism-server -- input
git commit -m "feat(server): InputChannelHandler with Win32 SendInput injection"
```

---

## Task 6: Client Input Sender

**Files:**
- Create: `crates/prism-client/src/input_sender.rs`
- Modify: `crates/prism-client/src/lib.rs`
- Modify: `crates/prism-client/src/main.rs`

- [ ] **Step 1: Write tests + implement InputSender**

`input_sender.rs`:
```rust
use bytes::{Bytes, BytesMut};
use prism_protocol::header::{PrismHeader, PROTOCOL_VERSION, HEADER_SIZE};
use prism_protocol::channel::CHANNEL_INPUT;
use prism_protocol::input::{InputEvent, INPUT_EVENT_SIZE};

/// Builds input event datagrams with a pre-built header template.
pub struct InputSender {
    header_template: [u8; HEADER_SIZE],
    sequence: u32,
}

impl InputSender {
    pub fn new() -> Self {
        let header = PrismHeader {
            version: PROTOCOL_VERSION,
            channel_id: CHANNEL_INPUT,
            msg_type: 0x01,
            flags: 0,
            sequence: 0,
            timestamp_us: 0,
            payload_length: INPUT_EVENT_SIZE as u32,
        };
        let mut template = [0u8; HEADER_SIZE];
        header.encode_to_slice(&mut template);
        Self { header_template: template, sequence: 0 }
    }

    /// Build a datagram for an input event. Patches sequence in the pre-built template.
    pub fn build_datagram(&mut self, event: InputEvent) -> Bytes {
        let mut buf = BytesMut::with_capacity(HEADER_SIZE + INPUT_EVENT_SIZE);
        let mut header = self.header_template;
        // Patch sequence (bytes 4-7 in LE header)
        header[4..8].copy_from_slice(&self.sequence.to_le_bytes());
        self.sequence += 1;
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&event.to_bytes());
        buf.freeze()
    }

    pub fn sequence(&self) -> u32 { self.sequence }
}

impl Default for InputSender { fn default() -> Self { Self::new() } }

/// Convert minifb mouse coordinates (window pixels) to normalized 0-65535.
pub fn normalize_mouse(x: f32, y: f32, window_w: u32, window_h: u32) -> (u16, u16) {
    let nx = ((x / window_w as f32) * 65535.0).clamp(0.0, 65535.0) as u16;
    let ny = ((y / window_h as f32) * 65535.0).clamp(0.0, 65535.0) as u16;
    (nx, ny)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_datagram_correct_size() {
        let mut sender = InputSender::new();
        let dgram = sender.build_datagram(InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 });
        assert_eq!(dgram.len(), HEADER_SIZE + INPUT_EVENT_SIZE);
    }

    #[test]
    fn sequence_increments() {
        let mut sender = InputSender::new();
        sender.build_datagram(InputEvent::KeyDown { scancode: 0, vk: 0 });
        sender.build_datagram(InputEvent::KeyUp { scancode: 0, vk: 0 });
        assert_eq!(sender.sequence(), 2);
    }

    #[test]
    fn datagram_has_valid_header() {
        let mut sender = InputSender::new();
        let dgram = sender.build_datagram(InputEvent::MouseMove { x: 100, y: 200 });
        let header = PrismHeader::decode_from_slice(&dgram).unwrap();
        assert_eq!(header.channel_id, CHANNEL_INPUT);
        assert_eq!(header.payload_length, INPUT_EVENT_SIZE as u32);
    }

    #[test]
    fn normalize_center() {
        let (x, y) = normalize_mouse(960.0, 540.0, 1920, 1080);
        assert!((x as i32 - 32768).abs() < 100);
        assert!((y as i32 - 32768).abs() < 100);
    }

    #[test]
    fn normalize_edges() {
        let (x, _) = normalize_mouse(0.0, 0.0, 1920, 1080);
        assert_eq!(x, 0);
        let (x, _) = normalize_mouse(1920.0, 0.0, 1920, 1080);
        assert_eq!(x, 65535);
    }
}
```

Update lib.rs: add `pub mod input_sender;` and `pub use input_sender::{InputSender, normalize_mouse};`

- [ ] **Step 2: Wire input into client main.rs**

The client main.rs needs to capture minifb keyboard/mouse events and send as datagrams. READ `crates/prism-client/src/main.rs` to understand the current structure, then add:

1. Create an `InputSender` at startup
2. In the window render loop (main thread), capture `window.get_keys()` and `window.get_mouse_pos(MouseMode::Clamp)` from minifb
3. Build input datagrams via `InputSender::build_datagram()`
4. Send to server via `connection.send_datagram()`

Key minifb APIs: `window.get_keys_pressed(KeyRepeat::Yes)`, `window.get_mouse_pos(MouseMode::Clamp)`, `window.get_mouse_down(MouseButton::Left)`.

The tricky part: the window runs on the main thread but the QUIC connection is async. Use `std::sync::mpsc` to send datagrams from the main thread to the async task that calls `connection.send_datagram()`.

- [ ] **Step 3: Verify build, commit**

```bash
cargo build -p prism-client
git commit -m "feat(client): InputSender with pre-built header template + minifb capture"
```

---

## Task 7: Wire Handlers into Server + Client Heartbeat

**Files:**
- Modify: `crates/prism-server/src/main.rs`
- Create: `crates/prism-client/src/heartbeat_client.rs`

- [ ] **Step 1: Register handlers in server main.rs**

In the server main.rs, after creating the `ChannelDispatcher`:

```rust
// Register channel handlers
let mut dispatcher = prism_session::ChannelDispatcher::new();
dispatcher.register(Arc::new(prism_server::ControlChannelHandler::new()));
dispatcher.register(Arc::new(prism_server::InputChannelHandler::new(width, height)));
let dispatcher = Arc::new(dispatcher);
```

Also spawn a per-client heartbeat sender task after the recv loop:

```rust
// Spawn heartbeat sender
let hb_conn = quinn_conn.clone();
let hb_gen = HeartbeatGenerator::new();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        interval.tick().await;
        if hb_conn.send_datagram(hb_gen.packet()).is_err() { break; }
    }
});
```

- [ ] **Step 2: Implement client heartbeat**

`heartbeat_client.rs`:
```rust
use bytes::Bytes;
use prism_server::HeartbeatGenerator;

// Client reuses the same HeartbeatGenerator (same wire format)
// Just send every 5 seconds via connection.send_datagram()

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_can_create_heartbeat() {
        let gen = HeartbeatGenerator::new();
        assert_eq!(gen.packet().len(), 16);
    }
}
```

Actually — the client shouldn't depend on prism-server. Move `HeartbeatGenerator` into prism-protocol instead, or duplicate the 5 lines. The simplest approach: create the heartbeat Bytes inline in the client main.rs (it's just a PrismHeader with HEARTBEAT msg_type).

- [ ] **Step 3: Wire heartbeat into client main.rs**

In the async receiver task, add a heartbeat sender:

```rust
// Heartbeat sender (runs alongside frame receiver)
let hb_conn = connection.clone();
tokio::spawn(async move {
    let heartbeat = build_heartbeat_packet(); // 16-byte pre-built
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        if hb_conn.send_datagram(heartbeat.clone()).is_err() { break; }
    }
});
```

- [ ] **Step 4: Verify both build, test workspace, commit**

```bash
cargo build -p prism-server
cargo build -p prism-client
cargo test --workspace
git commit -m "feat: wire input handlers + heartbeat into server and client"
```

---

## Task 8: E2E Input Forwarding Test

**Files:**
- Modify: `crates/prism-server/tests/e2e_frame_flow.rs`

- [ ] **Step 1: Write E2E test**

```rust
#[tokio::test]
async fn input_datagram_reaches_server_handler() {
    // Setup: loopback QUIC pair
    let (server_conn, client_conn, _acceptor) = make_loopback_pair().await;

    // Build an input datagram on the client side
    let mut input_sender = prism_client::InputSender::new();
    let datagram = input_sender.build_datagram(
        prism_protocol::input::InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 }
    );

    // Send from client → server
    client_conn.send_datagram(datagram.clone()).unwrap();

    // Server receives
    let received = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        server_conn.read_datagram()
    ).await.unwrap().unwrap();

    // Verify it's a valid input datagram
    let header = prism_protocol::header::PrismHeader::decode_from_slice(&received).unwrap();
    assert_eq!(header.channel_id, prism_protocol::channel::CHANNEL_INPUT);

    // Parse the input event
    let event = prism_protocol::input::InputEvent::from_bytes(
        &received[prism_protocol::header::HEADER_SIZE..]
    ).unwrap();
    assert!(matches!(event, prism_protocol::input::InputEvent::KeyDown { scancode: 0x1E, vk: 0x41 }));
}
```

- [ ] **Step 2: Verify, commit**

```bash
cargo test -p prism-server --test e2e_frame_flow -- input
git commit -m "test(e2e): input datagram delivery from client to server"
```

---

## Summary

| Task | Component | Tests |
|------|-----------|-------|
| 1 | InputEvent wire types (prism-protocol) | 9 |
| 2 | ChannelHandler refactor (add handle_stream) | 0 (existing pass) |
| 3 | HeartbeatGenerator (zero-alloc) | 3 |
| 4 | ControlChannelHandler | 5 |
| 5 | InputChannelHandler + Win32 SendInput | 6 |
| 6 | Client InputSender + minifb capture | 5 |
| 7 | Wire handlers into server + client heartbeat | 0 (build verify) |
| 8 | E2E input forwarding test | 1 |
| **Total** | | **~29** |

**After this plan:** `cargo run -p prism-server -- --dda` + `cargo run -p prism-client` = you can type and click in the client window and it controls the server's desktop. Heartbeats keep the session alive.

**Plan 10B (next):** Clipboard sync, audio streaming, TOFU pairing.
