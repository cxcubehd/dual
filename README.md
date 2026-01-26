# dual

![Screenshot](image.png)

> **⚠️ Work In Progress**: This project is currently under active development. Features and APIs are subject to change.

A "barebone" 3D game demo written in Rust, featuring a custom-built engine in the style of old-school shooters like Quake or CS 1.6.

This project deliberately avoids using general-purpose game engines (like Bevy or Godot) in favor of a low-level implementation using `wgpu` for graphics and `winit` for window management. It serves as a study in building a specialized 3D renderer and game loop from scratch.

## ✨ Features

- **Custom Graphics Engine**: Built on top of modern `wgpu` (WebGPU) architecture.
- **Old-school Movement**: Classic FPS controls with "noclip"-style free flying camera.
- **High Performance**: Written in pure Rust with minimal overhead.
- **Raw Input**: Native keyboard and mouse handling via `winit`.
- **Cross-Platform**: Runs on Linux (Wayland/X11), Windows, and macOS (Vulkan/Metal/DX12).

## 🎮 Controls

| Action | Key / Input |
|:---|:---|
| **Move** | `W`, `A`, `S`, `D` |
| **Fly Up** | `Space` |
| **Fly Down** | `Ctrl` |
| **Look** | Mouse |
| **Sprint** | `Shift` (Hold) |
| **Release Mouse** | `Esc` |
| **Toggle Fullscreen** | `F11` |
| **Quit** | `Shift` + `F12` |

## 🛠️ Tech Stack

- **[Rust](https://www.rust-lang.org/)**: Systems programming language.
- **[wgpu](https://wgpu.rs/)**: Safe, portable, cross-platform graphics API (WebGPU).
- **[winit](https://github.com/rust-windowing/winit)**: Window handling and input events.
- **[glam](https://github.com/bitshifter/glam-rs)**: Fast linear algebra libraries (vectors, matrices).
- **[tokio](https://tokio.rs/)**: Asynchronous runtime.

## 🚀 Getting Started

Ensure you have [Rust installed](https://rustup.rs/).

1. **Clone the repository:**

   ```bash
   git clone https://github.com/your-username/dual.git
   cd dual
   ```

2. **Run the project:**

   ```bash
   cargo run --release
   ```

   *Note: First compilation may take a moment to build dependencies.*

3. **Development Build:**

   For faster compile times during development:
   ```bash
   cargo run
   ```

## 📝 License

This project is open source and available under the MIT License.
