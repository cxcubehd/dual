use crate::vertex::Vertex;

// Cube vertices with different colors per vertex
pub const VERTICES: &[Vertex] = &[
    // Front face (red-ish gradient)
    Vertex { position: [-0.5, -0.5,  0.5], color: [1.0, 0.0, 0.0] },
    Vertex { position: [ 0.5, -0.5,  0.5], color: [1.0, 0.5, 0.0] },
    Vertex { position: [ 0.5,  0.5,  0.5], color: [1.0, 1.0, 0.0] },
    Vertex { position: [-0.5,  0.5,  0.5], color: [0.5, 1.0, 0.0] },
    // Back face (blue-ish gradient)
    Vertex { position: [-0.5, -0.5, -0.5], color: [0.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5, -0.5], color: [0.0, 0.5, 1.0] },
    Vertex { position: [ 0.5,  0.5, -0.5], color: [0.0, 1.0, 1.0] },
    Vertex { position: [-0.5,  0.5, -0.5], color: [0.5, 0.0, 1.0] },
];

pub const INDICES: &[u16] = &[
    // Front
    0, 1, 2, 2, 3, 0,
    // Back
    5, 4, 7, 7, 6, 5,
    // Top
    3, 2, 6, 6, 7, 3,
    // Bottom
    4, 5, 1, 1, 0, 4,
    // Right
    1, 5, 6, 6, 2, 1,
    // Left
    4, 0, 3, 3, 7, 4,
];
