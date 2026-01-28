use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub aspect: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    const PITCH_LIMIT: f64 = 89.9999_f64.to_radians();

    pub fn new(aspect: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, -3.0),
            yaw: 0.0,
            pitch: 0.0,
            aspect,
            fov: 90.0_f32.to_radians(),
            near: 0.01,
            far: 100.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        let (yaw, pitch) = (self.yaw as f32, self.pitch as f32);

        Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
        .normalize()
    }

    pub fn forward_xz(&self) -> Vec3 {
        let (sin, cos) = (self.yaw as f32).sin_cos();
        Vec3::new(sin, 0.0, cos)
    }

    pub fn right_xz(&self) -> Vec3 {
        let (sin, cos) = (self.yaw as f32).sin_cos();
        Vec3::new(cos, 0.0, -sin)
    }

    pub fn rotate(&mut self, delta_yaw: f64, delta_pitch: f64) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-Self::PITCH_LIMIT, Self::PITCH_LIMIT);
    }

    pub fn view_projection(&self) -> Mat4 {
        let view = Mat4::look_at_lh(self.position, self.position + self.forward(), Vec3::Y);
        let proj = Mat4::perspective_lh(self.fov, self.aspect, self.near, self.far);
        proj * view
    }

    /// View-projection matrix with translation removed (for skybox rendering).
    pub fn view_projection_no_translation(&self) -> Mat4 {
        // Create view matrix looking from origin
        let view = Mat4::look_at_lh(Vec3::ZERO, self.forward(), Vec3::Y);
        let proj = Mat4::perspective_lh(self.fov, self.aspect, self.near, self.far);
        proj * view
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    view_proj_no_translation: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn from_camera(camera: &Camera) -> Self {
        Self {
            view_proj: camera.view_projection().to_cols_array_2d(),
            view_proj_no_translation: camera.view_projection_no_translation().to_cols_array_2d(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}
