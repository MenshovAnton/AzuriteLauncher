# Azurite Launcher

A powerful Minecraft launcher that combines modern features with an interface similar to the official launcher from the 1.10 era. Experience the nostalgia of the old launcher with features similar to Prism.

## Tech Stack
Frontend:
- React
- TypeScript
- Tauri

Backend:
- Rust
- Tokio
- Reqwest

## Planned features
- Minimalistic nostalgia UI (from time Minecraft 1.10)
- Mod/Modpacks Manager
- Game instances (Containers)
- Basic security checks
- Quick play support

## Roadmap
- [x] Auto game download
- [x] Launch game
- [ ] Instances
- [ ] Auto Java download
- [ ] Microsoft auth
- [ ] Mod loaders support

## Implemented Minecraft Systems

- Version manifest parser
- Assets downloader
- Libraries resolver
- Native extraction
- Java classpath builder
- SHA1 validation

## Status
In Development. Early Stage.

## Known issues
- Linux and macOS builds are currently untested.

## Installation & Build

```bash
git clone https://github.com/MenshovAnton/AzuriteLauncher
cd AzuriteLauncher
pnpm install
pnpm tauri dev
```

## Project Structure

```text
src-tauri/
├─ capabilities/
├─ gen/
├─ icons/
├─ src/
│  ├─ lib.rs 
│  ├─ main.rs
│  ├─ launch_minecraft.rs    # game launching logic
│  ├─ minecraft_manager.rs   # game downloading logic
│  └─ path_manager.rs        # paths manager


src/
├─ assets/
├─ App.css
├─ App.tsx
├─ main.tsx
└─ vite-env.d.ts
```
## License ![GitHub License](https://img.shields.io/github/license/MenshovAnton/AzuriteLauncher)
This project is licensed under the GNU GPL v3.
