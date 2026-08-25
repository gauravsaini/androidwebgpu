//! Synthetic Sensor Motion Generator for realistic physical dynamics emulation.

use std::f32::consts::PI;

/// Earth standard gravity acceleration constant in m/s^2 (Android standard: 9.80665).
pub const EARTH_GRAVITY: f32 = 9.80665;

/// Synthetic motion generator for virtual accelerometer and gyroscope.
#[derive(Debug, Clone)]
pub struct SensorMotionGenerator {
    shake_frequency_hz: f32,
    shake_amplitude: f32,
    rotation_frequency_hz: f32,
    rotation_amplitude: f32,
}

impl Default for SensorMotionGenerator {
    fn default() -> Self {
        Self {
            shake_frequency_hz: 2.0, // 2 Hz shaking
            shake_amplitude: 4.0,    // 4 m/s^2 amplitude
            rotation_frequency_hz: 1.0, // 1 Hz roll/pitch
            rotation_amplitude: 1.57,  // ~90 deg/s in rad/s
        }
    }
}

impl SensorMotionGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_params(
        shake_frequency_hz: f32,
        shake_amplitude: f32,
        rotation_frequency_hz: f32,
        rotation_amplitude: f32,
    ) -> Self {
        Self {
            shake_frequency_hz,
            shake_amplitude,
            rotation_frequency_hz,
            rotation_amplitude,
        }
    }

    /// Generate 3-axis Accelerometer vector (x, y, z) in m/s^2 at given time (seconds).
    /// Default orientation: upright portrait (gravity = 9.81 on Y or Z axis).
    pub fn generate_accelerometer(&self, t_sec: f32, shake: bool) -> (f32, f32, f32) {
        let mut x = 0.0;
        let y = EARTH_GRAVITY;
        let mut z = 0.0;

        if shake {
            let phase = 2.0 * PI * self.shake_frequency_hz * t_sec;
            x += self.shake_amplitude * phase.sin();
            z += (self.shake_amplitude * 0.5) * phase.cos();
        }

        (x, y, z)
    }

    /// Generate 3-axis Gyroscope angular velocity vector (wx, wy, wz) in rad/s at given time (seconds).
    pub fn generate_gyroscope(&self, t_sec: f32, rotate: bool) -> (f32, f32, f32) {
        if !rotate {
            return (0.0, 0.0, 0.0);
        }

        let phase = 2.0 * PI * self.rotation_frequency_hz * t_sec;
        let wx = self.rotation_amplitude * phase.cos();
        let wy = (self.rotation_amplitude * 0.5) * phase.sin();
        let wz = (self.rotation_amplitude * 0.2) * (phase * 2.0).sin();

        (wx, wy, wz)
    }
}
