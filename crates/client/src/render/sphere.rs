use super::vertex::Vertex;
use std::f32::consts::PI;

pub fn create_sphere_mesh(radius: f32, stacks: u32, sectors: u32) -> (Vec<Vertex>, Vec<u16>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let sector_step = 2.0 * PI / sectors as f32;
    let stack_step = PI / stacks as f32;

    for i in 0..=stacks {
        let stack_angle = PI / 2.0 - i as f32 * stack_step; // from pi/2 to -pi/2
        let xy = radius * stack_angle.cos();
        let z = radius * stack_angle.sin();

        for j in 0..=sectors {
            let sector_angle = j as f32 * sector_step; // from 0 to 2pi

            let x = xy * sector_angle.cos();
            let y = xy * sector_angle.sin();

            // Distinct color for sphere (green/teal gradient)
            let color = [
                0.0,
                0.5 + (stack_angle / PI),
                0.5 + (sector_angle / (2.0 * PI)),
            ];

            vertices.push(Vertex {
                position: [x, z, y], // Swap Y and Z for Y-up?
                // Wait, in this engine Y seems to be up (gravity -9.81 in Y).
                // In generic math: x=r*cos(stack)*cos(sector), y=r*cos(stack)*sin(sector), z=r*sin(stack) is Z-up.
                // For Y-up: x=r*cos(stack)*cos(sector), z=r*cos(stack)*sin(sector), y=r*sin(stack).
                // Let's use Y-up.
                // x = xy * cos(sector)
                // z = xy * sin(sector)
                // y = radius * sin(stack)
                color,
            });
        }
    }

    // Indices
    for i in 0..stacks {
        let k1 = i * (sectors + 1);
        let k2 = k1 + sectors + 1;

        for j in 0..sectors {
            if i != 0 {
                indices.push((k1 + j) as u16);
                indices.push((k2 + j) as u16);
                indices.push((k1 + j + 1) as u16);
            }

            if i != (stacks - 1) {
                indices.push((k1 + j + 1) as u16);
                indices.push((k2 + j) as u16);
                indices.push((k2 + j + 1) as u16);
            }
        }
    }

    (vertices, indices)
}
