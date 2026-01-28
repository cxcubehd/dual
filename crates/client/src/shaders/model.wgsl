// Model shader with texture support

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct ModelUniform {
    transform: mat4x4<f32>,
};

@group(2) @binding(0)
var<uniform> model: ModelUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) world_normal: vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_position = model.transform * vec4<f32>(in.position, 1.0);
    out.clip_position = camera.view_proj * world_position;
    out.tex_coords = in.tex_coords;
    // Note: Normal transformation should use the inverse transpose of the model matrix 
    // to handle non-uniform scaling correctly. For now, we assume uniform scaling.
    // We only take the rotation part (upper 3x3).
    let normal_matrix = mat3x3<f32>(
        model.transform[0].xyz,
        model.transform[1].xyz,
        model.transform[2].xyz
    );
    out.world_normal = normal_matrix * in.normal;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let diffuse_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    
    // Simple directional lighting
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let normal = normalize(in.world_normal);
    let diffuse = max(dot(normal, light_dir), 0.0);
    
    // Ambient + diffuse lighting
    let ambient = 0.3;
    let lit_color = diffuse_color.rgb * (ambient + diffuse * 0.7);
    
    return vec4<f32>(lit_color, diffuse_color.a);
}
