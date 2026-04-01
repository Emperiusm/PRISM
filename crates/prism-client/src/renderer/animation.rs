// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pure-math animation system — spring/ease curves, batched AnimationPool.
//!
//! No GPU dependencies. Designed for cache-friendly batched ticking.

/// Easing curve variant applied during animation interpolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EaseCurve {
    Linear,
    EaseOut,
    EaseIn,
    /// Damped spring oscillation (omega=8, zeta=0.5) — overshoots before settling.
    Spring,
}

/// A single animatable f32 value that interpolates toward a target over time.
pub struct Animation {
    value: f32,
    target: f32,
    curve: EaseCurve,
    duration_ms: f32,
    elapsed_ms: f32,
    start_value: f32,
}

impl Animation {
    /// Create a new animation starting at 0.0 targeting 0.0.
    pub fn new(curve: EaseCurve, duration_ms: f32) -> Self {
        Self {
            value: 0.0,
            target: 0.0,
            curve,
            duration_ms,
            elapsed_ms: 0.0,
            start_value: 0.0,
        }
    }

    /// Update the target. If it changed, records current value as start and resets elapsed.
    pub fn set_target(&mut self, target: f32) {
        if (target - self.target).abs() > f32::EPSILON {
            self.start_value = self.value;
            self.elapsed_ms = 0.0;
            self.target = target;
        }
    }

    /// Advance time by `dt_ms` milliseconds and recompute the interpolated value.
    pub fn tick(&mut self, dt_ms: f32) {
        if self.is_dormant() {
            return;
        }

        self.elapsed_ms = (self.elapsed_ms + dt_ms).min(self.duration_ms);
        let t = if self.duration_ms > 0.0 {
            self.elapsed_ms / self.duration_ms
        } else {
            1.0
        };

        let eased = apply_curve(self.curve, t);
        self.value = self.start_value + (self.target - self.start_value) * eased;
    }

    /// Current interpolated value.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// True when the animation is essentially at its target (within 0.001).
    pub fn is_dormant(&self) -> bool {
        (self.value - self.target).abs() < 0.001
    }

    /// Current target value.
    pub fn target(&self) -> f32 {
        self.target
    }
}

/// Apply an easing curve to a normalized time `t` in [0, 1].
fn apply_curve(curve: EaseCurve, t: f32) -> f32 {
    match curve {
        EaseCurve::Linear => t,
        EaseCurve::EaseOut => 1.0 - (1.0 - t).powi(3),
        EaseCurve::EaseIn => t.powi(3),
        EaseCurve::Spring => {
            if t >= 1.0 {
                return 1.0;
            }
            let omega: f32 = 8.0;
            let zeta: f32 = 0.5;
            let time = t * 1.0; // t is already normalized 0..1 mapped to full duration
            // Use t directly as "normalized seconds" for the spring formula
            let decay = (-zeta * omega * time).exp();
            let freq = (1.0 - zeta * zeta).sqrt() * omega * time;
            1.0 - decay * freq.cos()
        }
    }
}

/// Opaque handle into an `AnimationPool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationId(usize);

/// Cache-friendly batch container for animations.
pub struct AnimationPool {
    animations: Vec<Animation>,
}

impl AnimationPool {
    /// Create an empty pool.
    pub fn new() -> Self {
        Self { animations: Vec::new() }
    }

    /// Add an animation and return its id.
    pub fn add(&mut self, curve: EaseCurve, duration_ms: f32) -> AnimationId {
        let id = AnimationId(self.animations.len());
        self.animations.push(Animation::new(curve, duration_ms));
        id
    }

    /// Update the target of a specific animation.
    pub fn set_target(&mut self, id: AnimationId, target: f32) {
        self.animations[id.0].set_target(target);
    }

    /// Current interpolated value for a specific animation.
    pub fn value(&self, id: AnimationId) -> f32 {
        self.animations[id.0].value()
    }

    /// Advance all animations by `dt_ms` milliseconds.
    pub fn tick_all(&mut self, dt_ms: f32) {
        for anim in &mut self.animations {
            anim.tick(dt_ms);
        }
    }

    /// True when every animation in the pool is dormant (or pool is empty).
    pub fn all_dormant(&self) -> bool {
        self.animations.iter().all(|a| a.is_dormant())
    }

    /// True when a specific animation is dormant.
    pub fn is_dormant(&self, id: AnimationId) -> bool {
        self.animations[id.0].is_dormant()
    }
}

impl Default for AnimationPool {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_out_starts_at_zero() {
        let anim = Animation::new(EaseCurve::EaseOut, 300.0);
        assert!((anim.value() - 0.0).abs() < 0.001);
    }

    #[test]
    fn ease_out_ends_at_one() {
        let mut anim = Animation::new(EaseCurve::EaseOut, 300.0);
        anim.set_target(1.0);
        anim.tick(300.0);
        assert!((anim.value() - 1.0).abs() < 0.001, "value was {}", anim.value());
    }

    #[test]
    fn spring_overshoots() {
        let mut anim = Animation::new(EaseCurve::Spring, 300.0);
        anim.set_target(1.0);
        // Tick to 40% of duration — spring should already be past midpoint due to overshoot
        anim.tick(120.0);
        assert!(anim.value() > 0.5, "spring value at 40% was {}", anim.value());
    }

    #[test]
    fn dormant_when_at_target() {
        // Starts at 0.0, target 0.0 → dormant immediately
        let anim = Animation::new(EaseCurve::Linear, 200.0);
        assert!(anim.is_dormant());
    }

    #[test]
    fn not_dormant_when_animating() {
        let mut anim = Animation::new(EaseCurve::Linear, 200.0);
        anim.set_target(1.0);
        // Has not been ticked yet — value still 0.0, target 1.0
        assert!(!anim.is_dormant());
    }

    #[test]
    fn linear_interpolation_midpoint() {
        let mut anim = Animation::new(EaseCurve::Linear, 200.0);
        anim.set_target(1.0);
        anim.tick(100.0); // 50% of duration
        assert!((anim.value() - 0.5).abs() < 0.001, "value was {}", anim.value());
    }

    #[test]
    fn batch_tick_all() {
        let mut pool = AnimationPool::new();
        let a = pool.add(EaseCurve::Linear, 200.0);
        let b = pool.add(EaseCurve::Linear, 200.0);
        pool.set_target(a, 1.0);
        pool.set_target(b, 1.0);
        pool.tick_all(100.0);
        assert!((pool.value(a) - 0.5).abs() < 0.001, "a was {}", pool.value(a));
        assert!((pool.value(b) - 0.5).abs() < 0.001, "b was {}", pool.value(b));
    }

    #[test]
    fn batch_all_dormant() {
        let pool = AnimationPool::new();
        assert!(pool.all_dormant());
    }

    #[test]
    fn batch_not_all_dormant() {
        let mut pool = AnimationPool::new();
        let a = pool.add(EaseCurve::Linear, 200.0);
        pool.set_target(a, 1.0);
        assert!(!pool.all_dormant());
    }
}
