use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    const PITCH_LIMIT: f32 = 1.5;

    pub fn new(aspect: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 0.0, -3.0),
            yaw: 0.0,
            pitch: 0.0,
            aspect,
            fov: 90.0_f32.to_radians(),
            near: 0.1,
            far: 100.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
        .normalize()
    }

    pub fn forward_xz(&self) -> Vec3 {
        let (sin, cos) = self.yaw.sin_cos();
        Vec3::new(sin, 0.0, cos)
    }

    pub fn right_xz(&self) -> Vec3 {
        let (sin, cos) = self.yaw.sin_cos();
        Vec3::new(cos, 0.0, -sin)
    }

    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-Self::PITCH_LIMIT, Self::PITCH_LIMIT);
    }

    pub fn view_projection(&self) -> Mat4 {
        let view = Mat4::look_at_lh(self.position, self.position + self.forward(), Vec3::Y);
        let proj = Mat4::perspective_lh(self.fov, self.aspect, self.near, self.far);
        proj * view
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn from_camera(camera: &Camera) -> Self {
        Self {
            view_proj: camera.view_projection().to_cols_array_2d(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts((self as *const Self).cast(), std::mem::size_of::<Self>())
        }
    }
}
