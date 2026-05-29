# Pumon CLI

Implemented MVP commands:

- `pumon init [output]`
- `pumon validate [config]`
- `pumon doctor [config]`
- `pumon start [config|script]`
- `pumon start --wait [config|script]`
- `pumon stop <name|all>`
- `pumon restart [config|script]`
- `pumon reload [config|script]`
- `pumon scale <config> <instances>`
- `pumon status [name]`
- `pumon prune`
- `pumon logs [name] [-n lines] [--follow]`
- `pumon watch [config|script]`
- `pumon daemon start|stop|status|ping|list`
- `pumon service install|start|stop|uninstall|status`
- `pumon tui [config]`

`pumon daemon start` launches `pumon daemon run <config>`, keeps desired apps reconciled, and exposes local IPC. On Unix platforms IPC uses a Unix socket under `PUMON_HOME/daemon`; on Windows it uses a localhost TCP listener address file.

When the daemon is reachable, normal management commands such as `start`, `stop`, `restart`, `reload`, `scale`, `status`, and `list` route through daemon IPC and update daemon desired state where applicable. If no daemon is reachable, they fall back to direct local process management.

`pumon prune` removes stale process records whose PID is no longer alive. It does not change daemon desired state.

For cluster apps, `scale` and `reload` use the cluster shim control channel when the app is already running, so the cluster master process can stay alive while workers are resized or replaced. The control channel is loopback-only, pid-checked, and token-checked through the local control address file. Non-cluster apps still use the supervisor restart path.

`pumon watch` honors `watch.paths`, `watch.include`, `watch.ignore`, `ignore_watch`, `watch.debounce_ms`, and `watch.reload`. If no app has watch enabled, the explicit `watch` command watches all resolved apps.

`pumon start --wait` supervises all resolved apps concurrently in the foreground, keeps them visible to `pumon list` and `pumon status`, and shuts them down cleanly on `Ctrl+C`.

`pumon tui [config]` opens an interactive terminal manager. It can list managed processes, tail logs, stop a selected process, and, when a config is loaded, start all apps or start/restart/reload/scale the selected app.
