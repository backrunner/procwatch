# Pumon

Pumon is a Rust-first Node.js process manager for JavaScript and TypeScript projects.

This repository currently contains the first MVP implementation:

- `pumon init`
- `pumon validate`
- `pumon doctor`
- `pumon prune`
- `pumon start`
- `pumon start --wait`
- `pumon stop`
- `pumon restart`
- `pumon reload`
- `pumon scale`
- `pumon status`
- `pumon list`
- `pumon logs`
- `pumon watch`
- `pumon daemon`
- `pumon service`
- `pumon tui`

Current implemented surface:

- JavaScript/TypeScript ecosystem config loading.
- Fork mode process start/stop/restart/list.
- Foreground supervision with restart, memory threshold, and interval restart.
- Cluster mode through the bundled Node cluster shim.
- Log capture, tailing, follow mode, and size-based rotation.
- Polling watch mode.
- User-level service definition generation.
- Background daemon wrapper for `start --wait`.
- Minimal terminal process dashboard.
- GitHub Release and npm wrapper scaffolding.
- Stale process pruning.
