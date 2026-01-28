# Assets Directory

This directory contains embedded assets that are included in the binary at compile time.

## Structure

- `models/` - GLTF/GLB 3D models
- `textures/` - Image textures (PNG, JPEG)
- `shaders/` - Additional shader files (optional, main shaders are in src/shaders/)

## Usage

Assets are loaded using the `Assets` module:

```rust
use crate::assets::Assets;

// Load text asset
let shader_source = Assets::load_string("shaders/custom.wgsl");

// Load binary asset
let model_bytes = Assets::load_bytes("models/cube.glb");

// Check if asset exists
if Assets::exists("textures/diffuse.png") {
    // ...
}
```
