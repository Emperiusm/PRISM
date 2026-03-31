use crate::histogram::HistogramSnapshot;

/// Type-erased snapshot of a MetricsRecorder. Used by the collector
/// to aggregate across subsystems without knowing their const generics.
#[derive(Debug, Clone)]
pub struct RecorderSnapshot {
    pub counters: Vec<u64>,
    pub gauges: Vec<i64>,
    pub histograms: Vec<HistogramSnapshot>,
    pub counter_names: Vec<&'static str>,
    pub gauge_names: Vec<&'static str>,
    pub histogram_names: Vec<&'static str>,
}

impl Default for RecorderSnapshot {
    fn default() -> Self {
        Self {
            counters: Vec::new(),
            gauges: Vec::new(),
            histograms: Vec::new(),
            counter_names: Vec::new(),
            gauge_names: Vec::new(),
            histogram_names: Vec::new(),
        }
    }
}
