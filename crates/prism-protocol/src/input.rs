// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2025-2026 Ehsan Khalid. All rights reserved.
// Licensed under the GNU Affero General Public License v3.0.
// Commercial licensing available — see LICENSE-COMMERCIAL.md.

/// Mouse button identifiers used in [`InputEvent::MouseDown`] and [`InputEvent::MouseUp`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MouseButton {
    Left = 0,
    Right = 1,
    Middle = 2,
    X1 = 3,
    X2 = 4,
}

impl MouseButton {
    /// Convert a raw `u8` tag to a `MouseButton`, returning `None` for unknown values.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            2 => Some(Self::Middle),
            3 => Some(Self::X1),
            4 => Some(Self::X2),
            _ => None,
        }
    }
}

/// Compact wire representation of an input event.
///
/// All variants encode to/from exactly [`INPUT_EVENT_SIZE`] bytes using a
/// leading tag byte followed by variant-specific payload bytes (little-endian),
/// padded with zeros to the fixed size.
///
/// | Tag | Variant                  | Payload bytes (offset 1-11)                    |
/// |-----|--------------------------|------------------------------------------------|
/// | 1   | `KeyDown`                | scancode LE u16, vk LE u16                     |
/// | 2   | `KeyUp`                  | scancode LE u16, vk LE u16                     |
/// | 3   | `TextInput`              | codepoint LE u32                               |
/// | 4   | `MouseMove`              | x LE u16, y LE u16                             |
/// | 5   | `MouseDown`              | button u8                                      |
/// | 6   | `MouseUp`                | button u8                                      |
/// | 7   | `MouseScroll`            | delta_x LE i16, delta_y LE i16                 |
/// | 8   | `MouseMoveRelative`      | dx LE i16, dy LE i16                           |
/// | 9   | `SetMouseMode`           | relative u8 (0=absolute, 1=relative)           |
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

/// Fixed byte length of every serialised [`InputEvent`].
pub const INPUT_EVENT_SIZE: usize = 12;

impl InputEvent {
    /// Serialise this event into a fixed-size 12-byte array (tag + LE payload + zeros).
    pub fn to_bytes(&self) -> [u8; INPUT_EVENT_SIZE] {
        let mut buf = [0u8; INPUT_EVENT_SIZE];
        match *self {
            Self::KeyDown { scancode, vk } => {
                buf[0] = 1;
                buf[1..3].copy_from_slice(&scancode.to_le_bytes());
                buf[3..5].copy_from_slice(&vk.to_le_bytes());
            }
            Self::KeyUp { scancode, vk } => {
                buf[0] = 2;
                buf[1..3].copy_from_slice(&scancode.to_le_bytes());
                buf[3..5].copy_from_slice(&vk.to_le_bytes());
            }
            Self::TextInput { codepoint } => {
                buf[0] = 3;
                buf[1..5].copy_from_slice(&codepoint.to_le_bytes());
            }
            Self::MouseMove { x, y } => {
                buf[0] = 4;
                buf[1..3].copy_from_slice(&x.to_le_bytes());
                buf[3..5].copy_from_slice(&y.to_le_bytes());
            }
            Self::MouseDown { button } => {
                buf[0] = 5;
                buf[1] = button as u8;
            }
            Self::MouseUp { button } => {
                buf[0] = 6;
                buf[1] = button as u8;
            }
            Self::MouseScroll { delta_x, delta_y } => {
                buf[0] = 7;
                buf[1..3].copy_from_slice(&delta_x.to_le_bytes());
                buf[3..5].copy_from_slice(&delta_y.to_le_bytes());
            }
            Self::MouseMoveRelative { dx, dy } => {
                buf[0] = 8;
                buf[1..3].copy_from_slice(&dx.to_le_bytes());
                buf[3..5].copy_from_slice(&dy.to_le_bytes());
            }
            Self::SetMouseMode { relative } => {
                buf[0] = 9;
                buf[1] = relative as u8;
            }
        }
        buf
    }

    /// Parse an [`InputEvent`] from a byte slice.
    ///
    /// Returns `None` if the slice is shorter than [`INPUT_EVENT_SIZE`] or the
    /// tag byte does not correspond to a known variant.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < INPUT_EVENT_SIZE {
            return None;
        }
        let tag = data[0];
        match tag {
            1 => {
                let scancode = u16::from_le_bytes([data[1], data[2]]);
                let vk = u16::from_le_bytes([data[3], data[4]]);
                Some(Self::KeyDown { scancode, vk })
            }
            2 => {
                let scancode = u16::from_le_bytes([data[1], data[2]]);
                let vk = u16::from_le_bytes([data[3], data[4]]);
                Some(Self::KeyUp { scancode, vk })
            }
            3 => {
                let codepoint = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
                Some(Self::TextInput { codepoint })
            }
            4 => {
                let x = u16::from_le_bytes([data[1], data[2]]);
                let y = u16::from_le_bytes([data[3], data[4]]);
                Some(Self::MouseMove { x, y })
            }
            5 => {
                let button = MouseButton::from_u8(data[1])?;
                Some(Self::MouseDown { button })
            }
            6 => {
                let button = MouseButton::from_u8(data[1])?;
                Some(Self::MouseUp { button })
            }
            7 => {
                let delta_x = i16::from_le_bytes([data[1], data[2]]);
                let delta_y = i16::from_le_bytes([data[3], data[4]]);
                Some(Self::MouseScroll { delta_x, delta_y })
            }
            8 => {
                let dx = i16::from_le_bytes([data[1], data[2]]);
                let dy = i16::from_le_bytes([data[3], data[4]]);
                Some(Self::MouseMoveRelative { dx, dy })
            }
            9 => {
                let relative = data[1] != 0;
                Some(Self::SetMouseMode { relative })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(event: InputEvent) -> InputEvent {
        let bytes = event.to_bytes();
        assert_eq!(bytes.len(), INPUT_EVENT_SIZE);
        InputEvent::from_bytes(&bytes).expect("roundtrip failed")
    }

    #[test]
    fn key_down_roundtrip() {
        let ev = InputEvent::KeyDown {
            scancode: 0x001C,
            vk: 0x000D,
        };
        assert_eq!(roundtrip(ev), ev);
    }

    #[test]
    fn key_up_roundtrip() {
        let ev = InputEvent::KeyUp {
            scancode: 0x001C,
            vk: 0x000D,
        };
        assert_eq!(roundtrip(ev), ev);
    }

    #[test]
    fn mouse_move_roundtrip() {
        let ev = InputEvent::MouseMove { x: 1920, y: 1080 };
        assert_eq!(roundtrip(ev), ev);
    }

    #[test]
    fn mouse_move_relative_roundtrip() {
        let ev = InputEvent::MouseMoveRelative { dx: -42, dy: 100 };
        assert_eq!(roundtrip(ev), ev);
    }

    #[test]
    fn text_input_roundtrip_emoji() {
        // U+1F600 GRINNING FACE
        let ev = InputEvent::TextInput { codepoint: 0x1F600 };
        assert_eq!(roundtrip(ev), ev);
    }

    #[test]
    fn mouse_scroll_roundtrip() {
        let ev = InputEvent::MouseScroll {
            delta_x: 0,
            delta_y: -3,
        };
        assert_eq!(roundtrip(ev), ev);
    }

    #[test]
    fn all_mouse_buttons_roundtrip() {
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::X1,
            MouseButton::X2,
        ] {
            let down = InputEvent::MouseDown { button };
            let up = InputEvent::MouseUp { button };
            assert_eq!(roundtrip(down), down, "MouseDown {button:?}");
            assert_eq!(roundtrip(up), up, "MouseUp {button:?}");
        }
    }

    #[test]
    fn set_mouse_mode_roundtrip() {
        let rel = InputEvent::SetMouseMode { relative: true };
        let abs = InputEvent::SetMouseMode { relative: false };
        assert_eq!(roundtrip(rel), rel);
        assert_eq!(roundtrip(abs), abs);
    }

    #[test]
    fn invalid_tag_returns_none() {
        let mut bytes = [0u8; INPUT_EVENT_SIZE];
        bytes[0] = 0xFF;
        assert!(InputEvent::from_bytes(&bytes).is_none());

        let mut bytes2 = [0u8; INPUT_EVENT_SIZE];
        bytes2[0] = 0;
        assert!(InputEvent::from_bytes(&bytes2).is_none());
    }

    #[test]
    fn too_short_returns_none() {
        let bytes = [1u8; INPUT_EVENT_SIZE - 1]; // one byte too few
        assert!(InputEvent::from_bytes(&bytes).is_none());

        assert!(InputEvent::from_bytes(&[]).is_none());
    }
}
