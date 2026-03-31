// Region classification for adaptive encoding.
// Task 5 will fully implement this module; RegionType is the minimal stub
// required by frame.rs.

/// Broad category of display region content used to select encode strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionType {
    /// Sharp edges, high contrast — benefits from lossless or high-QP encode.
    Text,
    /// Motion content — use video codec.
    Video,
    /// No recent change — can skip or send Unchanged.
    Static,
    /// Classification not yet determined.
    Uncertain,
}
