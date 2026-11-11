//! Keyframe animation: clips with property tracks and timed markers (events).
//!
//! A clip animates entity properties over time and fires markers at exact instants, which is
//! how sounds, script events and camera cuts stay in sync with the motion. Clips are used both
//! for real-time animation (doors, platforms, effects started from a script) and for cutscenes.

use crate::scene::Scene;
use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Property {
    Position,
    Rotation,
    Scale,
    Color,
    /// Camera field of view (uses the first component).
    Fov,
}

impl Property {
    pub const ALL: [Property; 5] =
        [Property::Position, Property::Rotation, Property::Scale, Property::Color, Property::Fov];
}

/// How the value moves from one key to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Ease {
    Linear,
    /// Slow start and slow end.
    #[default]
    Smooth,
    /// Holds the value until the next key.
    Step,
    /// Starts slowly, arrives fast.
    EaseIn,
    /// Leaves fast, arrives slowly.
    EaseOut,
    /// Passes the target a little and comes back.
    Overshoot,
    /// Bounces on arrival, like a ball on the floor.
    Bounce,
}

impl Ease {
    pub const ALL: [Ease; 7] =
        [Ease::Smooth, Ease::Linear, Ease::Step, Ease::EaseIn, Ease::EaseOut, Ease::Overshoot, Ease::Bounce];

    /// Maps the progress between two keys (0 to 1) to how much of the way is covered.
    pub fn apply(self, k: f32) -> f32 {
        match self {
            Ease::Linear => k,
            Ease::Smooth => k * k * (3.0 - 2.0 * k),
            Ease::Step => 0.0,
            Ease::EaseIn => k * k * k,
            Ease::EaseOut => 1.0 - (1.0 - k).powi(3),
            Ease::Overshoot => {
                let c = 1.70158;
                1.0 + (c + 1.0) * (k - 1.0).powi(3) + c * (k - 1.0).powi(2)
            }
            Ease::Bounce => {
                let (n, d) = (7.5625, 2.75);
                if k < 1.0 / d {
                    n * k * k
                } else if k < 2.0 / d {
                    let k = k - 1.5 / d;
                    n * k * k + 0.75
                } else if k < 2.5 / d {
                    let k = k - 2.25 / d;
                    n * k * k + 0.9375
                } else {
                    let k = k - 2.625 / d;
                    n * k * k + 0.984375
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub t: f32,
    pub value: [f32; 3],
    /// Applies to the stretch that starts at this key.
    #[serde(default)]
    pub ease: Ease,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub entity: u64,
    pub property: Property,
    pub keys: Vec<Key>,
}

impl Track {
    /// Value at time `t`, or `None` for a track without keys.
    pub fn sample(&self, t: f32) -> Option<[f32; 3]> {
        let first = self.keys.first()?;
        if t <= first.t {
            return Some(first.value);
        }
        for pair in self.keys.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if t < b.t {
                let span = (b.t - a.t).max(1e-6);
                let mut k = ((t - a.t) / span).clamp(0.0, 1.0);
                k = a.ease.apply(k);
                return Some(Vec3::from_array(a.value).lerp(Vec3::from_array(b.value), k).to_array());
            }
        }
        self.keys.last().map(|k| k.value)
    }

    /// Adds a key, or replaces the one already at that time. Returns its index.
    pub fn set_key(&mut self, t: f32, value: [f32; 3]) -> usize {
        if let Some(i) = self.keys.iter().position(|k| (k.t - t).abs() < 1e-3) {
            self.keys[i].value = value;
            return i;
        }
        let i = self.keys.iter().position(|k| k.t > t).unwrap_or(self.keys.len());
        self.keys.insert(i, Key { t, value, ease: Ease::default() });
        i
    }

    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    }
}

/// Something that happens at an exact instant of a clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Runs `on <event>(arg)` in the entity's script (`arg` is omitted when empty).
    Send { entity: u64, event: String, #[serde(default)] arg: String },
    /// Plays a sound file from the project.
    Sound { file: String, #[serde(default = "one")] volume: f32 },
    /// Switches the game to another camera (a camera cut).
    Camera { entity: u64 },
    /// Shows text on the game screen.
    Hud { text: String },
    /// Runs `on <event>(arg)` in every script of the scene.
    Broadcast { event: String, #[serde(default)] arg: String },
    /// Shows a quick time event; the clip waits for the player's answer.
    /// The result reaches every script as `on qte_success(id)` or `on qte_fail(id)`.
    Qte {
        id: String,
        #[serde(default)]
        kind: crate::qte::QteKind,
        /// One key, or several separated by spaces for a sequence.
        keys: String,
        #[serde(default = "ten")]
        count: u32,
        #[serde(default = "one")]
        hold: f32,
        seconds: f32,
    },
    /// Starts another clip.
    Play { clip: String },
    /// Changes the speed of the game (1 = normal, 0.25 = slow motion).
    TimeScale { scale: f32 },
}

fn ten() -> u32 {
    10
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub t: f32,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub name: String,
    pub duration: f32,
    #[serde(default)]
    pub looping: bool,
    /// Starts by itself when the game starts.
    #[serde(default)]
    pub autoplay: bool,
    /// Cutscene: the player's input is blocked and black bars are shown while it plays.
    #[serde(default)]
    pub cutscene: bool,
    #[serde(default)]
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub markers: Vec<Marker>,
}

impl Clip {
    pub fn new(name: impl Into<String>) -> Clip {
        Clip {
            name: name.into(),
            duration: 3.0,
            looping: false,
            autoplay: false,
            cutscene: false,
            tracks: vec![],
            markers: vec![],
        }
    }

    pub fn track_mut(&mut self, entity: u64, property: Property) -> &mut Track {
        let i = match self.tracks.iter().position(|t| t.entity == entity && t.property == property) {
            Some(i) => i,
            None => {
                self.tracks.push(Track { entity, property, keys: vec![] });
                self.tracks.len() - 1
            }
        };
        &mut self.tracks[i]
    }

    /// Writes every track's value at time `t` into the scene.
    pub fn apply(&self, scene: &mut Scene, t: f32) {
        for track in &self.tracks {
            let Some(v) = track.sample(t) else { continue };
            let Some(e) = scene.get_mut(track.entity) else { continue };
            match track.property {
                Property::Position => e.position = Vec3::from_array(v),
                Property::Rotation => e.rotation = Vec3::from_array(v),
                Property::Scale => e.scale = Vec3::from_array(v),
                Property::Color => {
                    if let Some(m) = &mut e.mesh {
                        m.color = v;
                    }
                }
                Property::Fov => {
                    if let Some(c) = &mut e.camera {
                        c.fov = v[0].clamp(5.0, 170.0);
                    }
                }
            }
        }
    }

    /// Markers in the half-open interval `(from, to]` (plus the ones at exactly 0 when `from` is 0
    /// and `include_start` is set), in time order.
    pub fn markers_between(&self, from: f32, to: f32, include_start: bool) -> Vec<&Marker> {
        let mut out: Vec<&Marker> = self
            .markers
            .iter()
            .filter(|m| (m.t > from && m.t <= to) || (include_start && m.t <= from))
            .collect();
        out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        out
    }
}

/// Current value of a property on an entity (what a new key records).
pub fn read_property(scene: &Scene, entity: u64, property: Property) -> Option<[f32; 3]> {
    let e = scene.get(entity)?;
    Some(match property {
        Property::Position => e.position.to_array(),
        Property::Rotation => e.rotation.to_array(),
        Property::Scale => e.scale.to_array(),
        Property::Color => e.mesh.as_ref()?.color,
        Property::Fov => [e.camera.as_ref()?.fov, 0.0, 0.0],
    })
}

/// A clip being played.
#[derive(Clone, Debug)]
pub struct Playing {
    pub clip: usize,
    pub t: f32,
    /// The clip has not been advanced yet (markers at time 0 still have to fire).
    pub fresh: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(keys: &[(f32, f32, Ease)]) -> Track {
        Track {
            entity: 1,
            property: Property::Position,
            keys: keys.iter().map(|(t, x, ease)| Key { t: *t, value: [*x, 0.0, 0.0], ease: *ease }).collect(),
        }
    }

    #[test]
    fn sampling_interpolates_and_clamps() {
        let tr = track(&[(1.0, 0.0, Ease::Linear), (3.0, 10.0, Ease::Linear)]);
        assert_eq!(tr.sample(0.0).unwrap()[0], 0.0);
        assert!((tr.sample(2.0).unwrap()[0] - 5.0).abs() < 1e-5);
        assert_eq!(tr.sample(9.0).unwrap()[0], 10.0);
        assert!(track(&[]).sample(1.0).is_none());
    }

    #[test]
    fn step_holds_and_smooth_eases() {
        let step = track(&[(0.0, 0.0, Ease::Step), (2.0, 10.0, Ease::Step)]);
        assert_eq!(step.sample(1.9).unwrap()[0], 0.0);
        assert_eq!(step.sample(2.0).unwrap()[0], 10.0);
        let smooth = track(&[(0.0, 0.0, Ease::Smooth), (2.0, 10.0, Ease::Smooth)]);
        assert!(smooth.sample(0.5).unwrap()[0] < 2.5, "starts slowly");
        assert!((smooth.sample(1.0).unwrap()[0] - 5.0).abs() < 1e-4);
    }

    #[test]
    fn set_key_keeps_order_and_replaces() {
        let mut tr = track(&[]);
        tr.set_key(2.0, [2.0, 0.0, 0.0]);
        tr.set_key(1.0, [1.0, 0.0, 0.0]);
        tr.set_key(3.0, [3.0, 0.0, 0.0]);
        assert_eq!(tr.set_key(2.0, [9.0, 0.0, 0.0]), 1);
        let times: Vec<f32> = tr.keys.iter().map(|k| k.t).collect();
        assert_eq!(times, [1.0, 2.0, 3.0]);
        assert_eq!(tr.keys[1].value[0], 9.0);
    }

    #[test]
    fn markers_fire_once_per_interval() {
        let mut clip = Clip::new("c");
        for t in [0.0, 1.0, 2.0] {
            clip.markers.push(Marker { t, action: Action::Hud { text: format!("{t}") } });
        }
        assert_eq!(clip.markers_between(0.0, 0.5, true).len(), 1);
        assert_eq!(clip.markers_between(0.5, 1.0, false).len(), 1);
        assert_eq!(clip.markers_between(1.0, 1.5, false).len(), 0);
        assert_eq!(clip.markers_between(1.5, 3.0, false).len(), 1);
    }

    #[test]
    fn clip_applies_tracks_to_the_scene() {
        let mut scene = Scene::default_scene();
        let cube = scene.find_by_name("Cube").unwrap();
        let mut clip = Clip::new("move");
        clip.track_mut(cube, Property::Position).set_key(0.0, [0.0, 0.5, 0.0]);
        clip.track_mut(cube, Property::Position).set_key(2.0, [4.0, 0.5, 0.0]);
        clip.track_mut(cube, Property::Color).set_key(0.0, [1.0, 0.0, 0.0]);
        clip.apply(&mut scene, 2.0);
        let e = scene.get(cube).unwrap();
        assert_eq!(e.position.x, 4.0);
        assert_eq!(e.mesh.as_ref().unwrap().color, [1.0, 0.0, 0.0]);
        // a track pointing at a deleted entity is ignored
        scene.remove(cube);
        clip.apply(&mut scene, 1.0);
    }
}
