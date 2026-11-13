//! Particles: sparks, smoke, dust, fire, embers, magic and explosions, started from scripts
//! with `fx.burst(position, "sparks")`. Simulated on the CPU, drawn as soft camera-facing sprites.

use glam::Vec3;

#[derive(Clone, Debug)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    pub age: f32,
    pub life: f32,
    pub size: (f32, f32),
    pub color: ([f32; 3], [f32; 3]),
    pub alpha: f32,
    pub gravity: f32,
    pub drag: f32,
    /// Adds light (sparks, fire) instead of covering what is behind (smoke, dust).
    pub glow: bool,
}

impl Particle {
    /// Size, colour and opacity right now.
    pub fn look(&self) -> (f32, [f32; 4]) {
        let k = (self.age / self.life).clamp(0.0, 1.0);
        let c = |i: usize| self.color.0[i] + (self.color.1[i] - self.color.0[i]) * k;
        // fade in quickly, fade out over the last part of the life
        let a = self.alpha * (k / 0.08).min(1.0) * (1.0 - k).powf(1.4);
        (self.size.0 + (self.size.1 - self.size.0) * k, [c(0), c(1), c(2), a])
    }
}

const MAX: usize = 6000;

/// Names a script can pass to `fx.burst`.
pub const PRESETS: &[&str] = &["sparks", "smoke", "dust", "fire", "embers", "magic", "explosion", "blood"];

pub struct Particles {
    pub list: Vec<Particle>,
    seed: u64,
}

impl Default for Particles {
    fn default() -> Self {
        Particles { list: vec![], seed: 0x2545F4914F6CDD1D }
    }
}

impl Particles {
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u64 << 24) as f32
    }
    fn dir(&mut self) -> Vec3 {
        loop {
            let v = Vec3::new(self.rand() * 2.0 - 1.0, self.rand() * 2.0 - 1.0, self.rand() * 2.0 - 1.0);
            let l = v.length();
            if l > 0.05 && l <= 1.0 {
                return v / l;
            }
        }
    }

    /// Starts `count` particles of a preset at `pos`. `tint` replaces the preset's colour;
    /// `toward` biases the motion (the normal of a surface that was hit, for example).
    pub fn burst(&mut self, pos: Vec3, preset: &str, count: u32, tint: Option<[f32; 3]>, toward: Vec3) -> Result<(), String> {
        let count = count.min(400) as usize;
        if self.list.len() + count > MAX {
            let over = self.list.len() + count - MAX;
            self.list.drain(0..over.min(self.list.len()));
        }
        if preset == "explosion" {
            self.burst(pos, "fire", (count as u32 * 2 / 3).max(1), tint, toward)?;
            self.burst(pos, "sparks", (count as u32 / 2).max(1), None, toward)?;
            return self.burst(pos, "smoke", (count as u32 / 2).max(1), None, Vec3::Y);
        }
        for _ in 0..count {
            let r = self.rand();
            let d = (self.dir() + toward * 0.9).normalize_or(Vec3::Y);
            let jitter = Vec3::new(self.rand() - 0.5, self.rand() - 0.5, self.rand() - 0.5) * 0.15;
            let mut p = match preset {
                "sparks" => Particle {
                    pos, vel: d * (4.0 + r * 7.0), age: 0.0, life: 0.25 + r * 0.35, size: (0.06, 0.02),
                    color: ([1.0, 0.85, 0.5], [1.0, 0.35, 0.05]), alpha: 1.0, gravity: -9.0, drag: 1.5, glow: true,
                },
                "smoke" => Particle {
                    pos: pos + jitter, vel: d * 0.6 + Vec3::Y * (0.6 + r * 0.8), age: 0.0, life: 1.8 + r * 1.6, size: (0.35, 1.6),
                    color: ([0.32, 0.32, 0.34], [0.2, 0.2, 0.22]), alpha: 0.55, gravity: 0.3, drag: 1.2, glow: false,
                },
                "dust" => Particle {
                    pos: pos + jitter, vel: Vec3::new(d.x, d.y.abs() * 0.4, d.z) * (1.0 + r * 2.0), age: 0.0, life: 0.8 + r * 0.8,
                    size: (0.2, 0.9), color: ([0.5, 0.44, 0.36], [0.4, 0.36, 0.3]), alpha: 0.45, gravity: -0.5, drag: 2.5, glow: false,
                },
                "fire" => Particle {
                    pos: pos + jitter * 2.0, vel: d * (1.5 + r * 2.5) + Vec3::Y * 1.5, age: 0.0, life: 0.4 + r * 0.5, size: (0.4, 0.9),
                    color: ([1.0, 0.75, 0.3], [0.8, 0.15, 0.03]), alpha: 0.9, gravity: 2.0, drag: 2.0, glow: true,
                },
                "embers" => Particle {
                    pos: pos + jitter, vel: Vec3::new(d.x * 0.3, 0.8 + r * 1.2, d.z * 0.3), age: 0.0, life: 1.0 + r * 1.2, size: (0.035, 0.015),
                    color: ([1.0, 0.6, 0.2], [0.9, 0.2, 0.05]), alpha: 1.0, gravity: 0.4, drag: 0.6, glow: true,
                },
                "magic" => Particle {
                    pos: pos + jitter * 3.0, vel: d * 0.8 + Vec3::Y * 0.6, age: 0.0, life: 0.9 + r * 0.8, size: (0.12, 0.02),
                    color: ([0.6, 0.95, 1.0], [0.2, 0.5, 1.0]), alpha: 1.0, gravity: 0.6, drag: 1.0, glow: true,
                },
                "blood" => Particle {
                    pos, vel: d * (1.5 + r * 2.5), age: 0.0, life: 0.5 + r * 0.4, size: (0.08, 0.05),
                    color: ([0.45, 0.02, 0.02], [0.25, 0.0, 0.0]), alpha: 0.95, gravity: -9.0, drag: 1.0, glow: false,
                },
                other => return Err(format!("unknown effect `{other}` (use {})", PRESETS.join(", "))),
            };
            if let Some(t) = tint {
                p.color = (t, [t[0] * 0.6, t[1] * 0.6, t[2] * 0.6]);
            }
            self.list.push(p);
        }
        Ok(())
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.list {
            p.age += dt;
            p.vel.y += p.gravity * dt;
            p.vel *= (1.0 - p.drag * dt).max(0.0);
            p.pos += p.vel * dt;
        }
        self.list.retain(|p| p.age < p.life);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bursts_move_fade_and_end() {
        let mut fx = Particles::default();
        fx.burst(Vec3::ZERO, "sparks", 50, None, Vec3::Y).unwrap();
        fx.burst(Vec3::ZERO, "smoke", 10, Some([1.0, 0.0, 0.0]), Vec3::Y).unwrap();
        assert_eq!(fx.list.len(), 60);
        assert!(fx.burst(Vec3::ZERO, "glitter", 1, None, Vec3::Y).is_err());
        fx.update(0.1);
        assert!(fx.list.iter().filter(|p| p.glow).all(|p| p.pos.length() > 0.1), "sparks fly out");
        let smoke = fx.list.iter().find(|p| !p.glow).unwrap();
        assert!(smoke.look().1[0] > smoke.look().1[1], "tinted red");
        for _ in 0..40 {
            fx.update(0.1);
        }
        assert!(fx.list.is_empty(), "everything ends");
        fx.burst(Vec3::ZERO, "explosion", 30, None, Vec3::Y).unwrap();
        assert!(fx.list.len() >= 40);
        // there is a cap, so a script in a loop cannot freeze the game
        for _ in 0..100 {
            fx.burst(Vec3::ZERO, "dust", 400, None, Vec3::Y).unwrap();
        }
        assert!(fx.list.len() <= MAX);
    }
}
