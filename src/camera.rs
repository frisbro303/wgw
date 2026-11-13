//! An orbit camera around the scene, which is fitted (centered and scaled) into the unit sphere.

use glam::{Mat4, Vec3};

pub const FOV: f32 = 30.0 * std::f32::consts::PI / 180.0;
const PITCH_LIMIT: f32 = 89.0 * std::f32::consts::PI / 180.0;

/// The axis that points up. The camera orbits a z-up world; other up axes are a cyclic relabeling of x, y, z,
/// which is a rotation, so handedness is kept.
#[derive(Clone, Copy)]
pub enum Up {
    X,
    Y,
    Z,
}

impl Up {
    pub fn parse(s: &str) -> Up {
        match s {
            "x" => Up::X,
            "y" => Up::Y,
            _ => Up::Z,
        }
    }

    fn map(self, v: Vec3) -> Vec3 {
        match self {
            Up::Z => v,
            Up::Y => Vec3::new(v.y, v.z, v.x),
            Up::X => Vec3::new(v.z, v.x, v.y),
        }
    }
}

/// Where the scene sits: its bounding sphere, which the model matrix maps onto the unit sphere.
#[derive(Clone, Copy)]
pub struct Fit {
    pub center: Vec3,
    pub radius: f32,
}

impl Fit {
    pub fn model(&self) -> Mat4 {
        Mat4::from_scale(Vec3::splat(1.0 / self.radius)) * Mat4::from_translation(-self.center)
    }
}

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub home: (f32, f32),
    pub up: Up,
    /// Radians per millisecond, for the glide after a drag.
    pub velocity: (f32, f32),
}

impl Camera {
    pub fn new() -> Self {
        let home = ((-60f32).to_radians(), 20f32.to_radians());
        Camera { yaw: home.0, pitch: home.1, zoom: 1.0, home, up: Up::Z, velocity: (0.0, 0.0) }
    }

    pub fn reset(&mut self) {
        self.yaw = self.home.0;
        self.pitch = self.home.1;
        self.zoom = 1.0;
        self.velocity = (0.0, 0.0);
    }

    pub fn turn(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.2, 4.0);
    }

    /// View and projection matrices, with the unit sphere fitting the narrower side of the viewport.
    pub fn matrices(&self, aspect: f32) -> (Mat4, Mat4) {
        let half = (FOV / 2.0).min(((FOV / 2.0).tan() * aspect).atan());
        let distance = 1.04 / half.sin() * self.zoom;
        let cp = self.pitch.cos();
        let eye = self.up.map(Vec3::new(cp * self.yaw.cos(), cp * self.yaw.sin(), self.pitch.sin()) * distance);
        let view = Mat4::look_at_rh(eye, Vec3::ZERO, self.up.map(Vec3::Z));
        let proj = Mat4::perspective_rh_gl(FOV, aspect, (distance - 1.5).max(distance * 0.02), distance + 1.5);
        (view, proj)
    }
}
