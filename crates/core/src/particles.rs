//! The fairy dust thrown on a tap: glass stars, beads and bubbles, as in TapEffect.swift.

use crate::random::Random;

/// The dust's jewel tones (RGB 0..1).
pub const HUES: [(f32, f32, f32); 5] = [
    (0.96, 0.30, 0.68),
    (0.60, 0.38, 0.98),
    (0.16, 0.62, 1.00),
    (0.10, 0.84, 0.70),
    (1.00, 0.70, 0.16),
];

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum TapEffect {
    Off,
    #[default]
    FairyDust,
    FairyDustSubtle,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Ring,
    Star,
    Bead,
    Bubble,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub kind: Kind,
    pub hue: usize,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub gravity: f32,
    pub size: f32,
    pub shrink: f32,
    pub spin: f32,
    pub angle: f32,
    pub age: f32,
    pub life: f32,
}

impl Particle {
    /// How visible it still is, 1 at birth to 0 at the end of its life.
    pub fn opacity(&self) -> f32 {
        (1.0 - self.age / self.life).clamp(0.0, 1.0).powf(0.8)
    }
}

pub struct Particles {
    pub live: Vec<Particle>,
    random: Random,
}

impl Default for Particles {
    fn default() -> Self {
        Particles { live: Vec::new(), random: Random::from_time() }
    }
}

struct Spec {
    kind: Kind,
    count: usize,
    speed: (f32, f32),
    gravity: f32,
    size: (f32, f32),
    shrink: f32,
    life: (f32, f32),
    spin: f32,
}

const SPECS: [Spec; 3] = [
    Spec { kind: Kind::Star, count: 3, speed: (120.0, 75.0), gravity: 120.0, size: (24.0, 11.0), shrink: 13.0, life: (1.1, 0.4), spin: 4.0 },
    Spec { kind: Kind::Bead, count: 10, speed: (100.0, 90.0), gravity: 130.0, size: (5.6, 2.8), shrink: 2.8, life: (0.85, 0.4), spin: 3.0 },
    Spec { kind: Kind::Bubble, count: 4, speed: (46.0, 44.0), gravity: 30.0, size: (12.0, 6.0), shrink: 3.4, life: (1.4, 0.6), spin: 2.0 },
];

impl Particles {
    pub fn with_seed(seed: u64) -> Self {
        Particles { live: Vec::new(), random: Random::new(seed) }
    }

    /// Throws a burst at (x, y). `unit` scales sizes and speeds: 1 on a 900 pt tall screen
    /// with the default hand size.
    pub fn burst(&mut self, effect: TapEffect, x: f32, y: f32, unit: f32) {
        // The subtle variant: fewer, slower, smaller, shorter-lived, fainter, no ring.
        let (count, speed, size, life) = match effect {
            TapEffect::Off => return,
            TapEffect::FairyDust => (1.0, 1.0, 1.0, 1.0),
            TapEffect::FairyDustSubtle => (0.3, 0.55, 0.72, 0.75),
        };
        if effect == TapEffect::FairyDust {
            self.live.push(Particle {
                kind: Kind::Ring, hue: 1, x, y, vx: 0.0, vy: 0.0, gravity: 0.0,
                size: 4.0 * unit, shrink: -150.0 * unit, spin: 0.0, angle: 0.0, age: 0.0, life: 0.5,
            });
        }
        let r = &mut self.random;
        for hue in 0..HUES.len() {
            for spec in &SPECS {
                let n = ((spec.count as f32 * count).round() as usize).max(1);
                for _ in 0..n {
                    let heading = r.next() * std::f32::consts::TAU;
                    let v = r.around(spec.speed.0, spec.speed.1).max(0.0) * unit * speed;
                    self.live.push(Particle {
                        kind: spec.kind,
                        hue,
                        x,
                        y,
                        vx: heading.cos() * v,
                        vy: heading.sin() * v,
                        gravity: spec.gravity * unit * speed,
                        size: r.around(spec.size.0, spec.size.1).max(1.0) * unit * size,
                        shrink: spec.shrink * unit * size,
                        spin: r.around(0.0, spec.spin),
                        angle: r.next() * std::f32::consts::TAU,
                        age: 0.0,
                        life: r.around(spec.life.0, spec.life.1).max(0.2) * life,
                    });
                }
            }
        }
    }

    pub fn step(&mut self, dt: f32) {
        for p in &mut self.live {
            p.age += dt;
            p.vy += p.gravity * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.size = (p.size - p.shrink * dt).max(0.0);
            p.angle += p.spin * dt;
        }
        self.live.retain(|p| p.age < p.life && p.size > 0.3);
    }

    pub fn clear(&mut self) {
        self.live.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_die_out() {
        let mut p = Particles::with_seed(7);
        p.burst(TapEffect::FairyDust, 100.0, 100.0, 1.0);
        assert!(p.live.len() > 50);
        for _ in 0..400 {
            p.step(1.0 / 120.0);
        }
        assert!(p.is_empty());
    }

    #[test]
    fn subtle_is_smaller_and_off_is_nothing() {
        let mut full = Particles::with_seed(1);
        full.burst(TapEffect::FairyDust, 0.0, 0.0, 1.0);
        let mut subtle = Particles::with_seed(1);
        subtle.burst(TapEffect::FairyDustSubtle, 0.0, 0.0, 1.0);
        assert!(subtle.live.len() * 2 < full.live.len());
        let mut off = Particles::with_seed(1);
        off.burst(TapEffect::Off, 0.0, 0.0, 1.0);
        assert!(off.is_empty());
    }
}
