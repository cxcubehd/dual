// Skybox shader - renders a cubemap-textured cube from inside.
//
// Cube vertices are generated procedurally. The view-projection matrix
// without translation keeps the skybox centered on the camera.

// ============================================================================
// Uniforms
// ============================================================================

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_proj_no_translation: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_skybox: texture_cube<f32>;

@group(1) @binding(1)
var s_skybox: sampler;

// ============================================================================
// Vertex shader
// ============================================================================

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_position: vec3<f32>,
}

/// Generates cube vertex positions for 36 vertices (6 faces × 2 triangles × 3 vertices).
fn get_cube_position(index: u32) -> vec3<f32> {
    var positions = array<vec3<f32>, 36>(
        // +X face
        vec3( 1.0, -1.0, -1.0), vec3( 1.0, -1.0,  1.0), vec3( 1.0,  1.0,  1.0),
        vec3( 1.0, -1.0, -1.0), vec3( 1.0,  1.0,  1.0), vec3( 1.0,  1.0, -1.0),
        // -X face
        vec3(-1.0, -1.0,  1.0), vec3(-1.0, -1.0, -1.0), vec3(-1.0,  1.0, -1.0),
        vec3(-1.0, -1.0,  1.0), vec3(-1.0,  1.0, -1.0), vec3(-1.0,  1.0,  1.0),
        // +Y face
        vec3(-1.0,  1.0, -1.0), vec3( 1.0,  1.0, -1.0), vec3( 1.0,  1.0,  1.0),
        vec3(-1.0,  1.0, -1.0), vec3( 1.0,  1.0,  1.0), vec3(-1.0,  1.0,  1.0),
        // -Y face
        vec3(-1.0, -1.0,  1.0), vec3( 1.0, -1.0,  1.0), vec3( 1.0, -1.0, -1.0),
        vec3(-1.0, -1.0,  1.0), vec3( 1.0, -1.0, -1.0), vec3(-1.0, -1.0, -1.0),
        // +Z face
        vec3(-1.0, -1.0,  1.0), vec3(-1.0,  1.0,  1.0), vec3( 1.0,  1.0,  1.0),
        vec3(-1.0, -1.0,  1.0), vec3( 1.0,  1.0,  1.0), vec3( 1.0, -1.0,  1.0),
        // -Z face
        vec3( 1.0, -1.0, -1.0), vec3( 1.0,  1.0, -1.0), vec3(-1.0,  1.0, -1.0),
        vec3( 1.0, -1.0, -1.0), vec3(-1.0,  1.0, -1.0), vec3(-1.0, -1.0, -1.0),
    );
    return positions[index];
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let position = get_cube_position(vertex_index);

    // Transform without translation to keep skybox centered on camera
    var clip_pos = camera.view_proj_no_translation * vec4(position, 1.0);
    
    // Force depth to maximum (z = w → normalized depth = 1.0)
    clip_pos.z = clip_pos.w;

    return VertexOutput(clip_pos, position);
}

// ============================================================================
// Fragment shader
// ============================================================================

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Flip Z for left-handed to cubemap coordinate conversion
    let sample_dir = vec3(in.local_position.x, in.local_position.y, -in.local_position.z);
    return textureSample(t_skybox, s_skybox, sample_dir);
}
