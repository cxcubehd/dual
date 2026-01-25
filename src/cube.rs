use crate::vertex::Vertex;

pub const VERTICES: &[Vertex] = &[
    Vertex { position: [-0.5, -0.5,  0.5], color: [1.0, 0.0, 0.0] },
    Vertex { position: [ 0.5, -0.5,  0.5], color: [1.0, 0.5, 0.0] },
    Vertex { position: [ 0.5,  0.5,  0.5], color: [1.0, 1.0, 0.0] },
    Vertex { position: [-0.5,  0.5,  0.5], color: [0.5, 1.0, 0.0] },
    Vertex { position: [-0.5, -0.5, -0.5], color: [0.0, 0.0, 1.0] },
    Vertex { position: [ 0.5, -0.5, -0.5], color: [0.0, 0.5, 1.0] },
    Vertex { position: [ 0.5,  0.5, -0.5], color: [0.0, 1.0, 1.0] },
    Vertex { position: [-0.5,  0.5, -0.5], color: [0.5, 0.0, 1.0] },
];

#[rustfmt::skip]
pub const INDICES: &[u16] = &[
    0, 2, 1, 0, 3, 2, // Front  (+Z)
    4, 5, 6, 4, 6, 7, // Back   (-Z)
    3, 6, 2, 3, 7, 6, // Top    (+Y)
    0, 1, 5, 0, 5, 4, // Bottom (-Y)
    1, 2, 6, 1, 6, 5, // Right  (+X)
    0, 4, 7, 0, 7, 3, // Left   (-X)
];
