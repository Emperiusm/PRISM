/// Client-side cursor prediction.
/// Renders cursor at local position for zero-latency feel.
/// Accepts server corrections when positions diverge.
pub struct CursorPredictor {
    /// Client's predicted cursor position (from local mouse events).
    local_x: f32,
    local_y: f32,
    /// Server's last reported cursor position.
    server_x: f32,
    server_y: f32,
    /// Correction threshold in pixels — snap to server when diverged beyond this.
    correction_threshold: f32,
    /// Whether the cursor is visible.
    visible: bool,
    corrections_applied: u32,
}

impl CursorPredictor {
    pub fn new(correction_threshold: f32) -> Self {
        Self {
            local_x: 0.0,
            local_y: 0.0,
            server_x: 0.0,
            server_y: 0.0,
            correction_threshold,
            visible: true,
            corrections_applied: 0,
        }
    }

    /// Update from local mouse movement (zero latency).
    pub fn update_local(&mut self, x: f32, y: f32) {
        self.local_x = x;
        self.local_y = y;
    }

    /// Update from server-reported position (arrives with 1-RTT delay).
    /// Returns true if a correction was applied (local prediction was wrong).
    pub fn update_server(&mut self, x: f32, y: f32, visible: bool) -> bool {
        self.server_x = x;
        self.server_y = y;
        self.visible = visible;

        let dx = (self.local_x - x).abs();
        let dy = (self.local_y - y).abs();

        if dx > self.correction_threshold || dy > self.correction_threshold {
            // Prediction diverged — snap to server position
            self.local_x = x;
            self.local_y = y;
            self.corrections_applied += 1;
            true
        } else {
            false
        }
    }

    /// Current display position (local prediction).
    pub fn display_position(&self) -> (f32, f32) {
        (self.local_x, self.local_y)
    }

    /// Divergence from server in pixels.
    pub fn divergence(&self) -> f32 {
        let dx = self.local_x - self.server_x;
        let dy = self.local_y - self.server_y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn corrections_applied(&self) -> u32 {
        self.corrections_applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_update_zero_latency() {
        let mut predictor = CursorPredictor::new(4.0);
        predictor.update_local(123.5, 456.7);
        assert_eq!(predictor.display_position(), (123.5, 456.7));
    }

    #[test]
    fn server_correction_when_diverged() {
        let mut predictor = CursorPredictor::new(4.0);
        predictor.update_local(0.0, 0.0);
        // Server reports position far from local
        let corrected = predictor.update_server(100.0, 200.0, true);
        assert!(corrected);
        assert_eq!(predictor.display_position(), (100.0, 200.0));
        assert_eq!(predictor.corrections_applied(), 1);
    }

    #[test]
    fn no_correction_when_close() {
        let mut predictor = CursorPredictor::new(4.0);
        predictor.update_local(100.0, 200.0);
        // Server reports position within threshold
        let corrected = predictor.update_server(101.0, 200.5, true);
        assert!(!corrected);
        // Still shows local position
        assert_eq!(predictor.display_position(), (100.0, 200.0));
        assert_eq!(predictor.corrections_applied(), 0);
    }

    #[test]
    fn divergence_calculation() {
        let mut predictor = CursorPredictor::new(4.0);
        predictor.update_local(3.0, 4.0);
        // Server at origin, local at (3, 4) → Euclidean distance = 5
        predictor.update_server(0.0, 0.0, true);
        // After correction (since 4.0 >= threshold 4.0), local snaps to server
        // So after correction, local = (0,0), server = (0,0), divergence = 0
        // Let's use a higher threshold to avoid correction
        let mut predictor2 = CursorPredictor::new(10.0);
        predictor2.update_local(3.0, 4.0);
        predictor2.update_server(0.0, 0.0, true);
        // No correction since |3| and |4| are both < 10
        let div = predictor2.divergence();
        assert!((div - 5.0).abs() < 1e-5, "expected divergence ~5.0, got {}", div);
    }

    #[test]
    fn visibility_from_server() {
        let mut predictor = CursorPredictor::new(4.0);
        assert!(predictor.is_visible());
        predictor.update_server(0.0, 0.0, false);
        assert!(!predictor.is_visible());
    }

    #[test]
    fn corrections_counted() {
        let mut predictor = CursorPredictor::new(4.0);
        // Each update_server call from far away increments counter
        predictor.update_local(0.0, 0.0);
        predictor.update_server(100.0, 0.0, true);
        assert_eq!(predictor.corrections_applied(), 1);

        // After snap, local is at server; move local far again
        predictor.update_local(0.0, 0.0);
        predictor.update_server(200.0, 0.0, true);
        assert_eq!(predictor.corrections_applied(), 2);

        predictor.update_local(0.0, 0.0);
        predictor.update_server(300.0, 0.0, true);
        assert_eq!(predictor.corrections_applied(), 3);
    }
}
