# Grid Roguelike

A GPU-accelerated grid-based roguelike game built with Rust, inspired by Dwarf Fortress.

## Features

- **GPU-Accelerated Rendering**: Uses OpenGL instanced rendering for efficient display of large grids
- **Interactive Camera**: Smooth pan and zoom controls
  - Click and drag to pan the view
  - Scroll wheel to zoom in/out at cursor position
- **Procedural Generation**: Perlin noise-based terrain generation
- **ECS Ready**: Built with `hecs` ECS library for future game entity management

## Architecture

- **SDL2** - Windowing and input handling
- **glow** - OpenGL bindings for GPU rendering
- **hecs** - Entity Component System for game logic (ready to use)
- **glam** - Vector and matrix math
- **noise** - Procedural terrain generation

## Project Structure

```
src/
├── main.rs       # Application entry point and event loop
├── camera.rs     # Camera system with pan/zoom
├── renderer.rs   # GPU-accelerated instanced renderer
├── grid.rs       # Grid world with procedural generation
└── tile.rs       # Tile types and properties
```

## Building and Running

### Prerequisites

```bash
# Arch Linux
sudo pacman -S rust sdl2

# Ubuntu/Debian
sudo apt install cargo libsdl2-dev

# macOS
brew install rust sdl2
```

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

## Controls

- **Left Click + Drag**: Pan the camera
- **Mouse Wheel**: Zoom in/out at cursor position
- **ESC**: Quit

## How It Works

### GPU Rendering

The renderer uses OpenGL instanced rendering to efficiently draw thousands of tiles:
- A single quad mesh is created (6 vertices)
- Instance data contains position and color for each visible tile
- GPU draws all tiles in a single draw call
- Only tiles within camera view are rendered

### Camera System

The camera provides smooth pan and zoom:
- Zoom-at-cursor keeps the world point under the mouse stationary
- Orthographic projection for perfect pixel rendering
- Frustum culling renders only visible tiles

### Future Extensions

This foundation is ready for:
- Adding entities (creatures, items) via the ECS
- Pathfinding and AI systems
- Player interaction and controls
- Save/load systems
- Advanced procedural generation
- Multi-layer rendering (terrain, entities, UI)

## Next Steps

To add gameplay:

1. **Define Components** in a new `components.rs`:
   - Position, Sprite, Health, etc.

2. **Create Entities** using hecs:
   ```rust
   let player = world.spawn((
       Position { x: 0, y: 0 },
       Sprite { color: Vec3::new(1.0, 1.0, 0.0) },
   ));
   ```

3. **Implement Systems** to update game logic:
   - Movement, AI, combat, etc.

4. **Render Entities** in the renderer alongside tiles

## License

MIT
