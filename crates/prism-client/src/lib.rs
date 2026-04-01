pub mod client_app;
pub mod connector;
pub mod frame_receiver;
pub mod input_sender;
pub mod audio_player;

// Re-exports
pub use client_app::{ClientApp, ClientConfig};
pub use connector::{ClientConnector, TlsMode};
pub use frame_receiver::{FrameStats, parse_display_datagram};
pub use input_sender::{InputSender, normalize_mouse};
pub use audio_player::AdaptiveJitterBuffer;
