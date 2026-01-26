use crate::vertex::Vertex;

pub const VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [1.0, 0.5, 0.0],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [1.0, 1.0, 0.0],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.5, 1.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.0, 0.0, 1.0],
    },
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.0, 0.5, 1.0],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.0, 1.0, 1.0],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.5, 0.0, 1.0],
    },
];

#[rustfmt::skip]
pub const INDICES: &[u16] = &[
    0, 1, 2, 0, 2, 3, // Front  (+Z)
    4, 6, 5, 4, 7, 6, // Back   (-Z)
    3, 2, 6, 3, 6, 7, // Top    (+Y)
    0, 5, 1, 0, 4, 5, // Bottom (-Y)
    1, 6, 2, 1, 5, 6, // Right  (+X)
    0, 7, 4, 0, 3, 7, // Left   (-X)
];
