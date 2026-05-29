# Promon CLI

Implemented MVP commands:

- `promon init [output]`
- `promon validate [config]`
- `promon doctor`
- `promon start [config|script]`
- `promon start --wait [config|script]`
- `promon stop <name|all>`
- `promon restart [config|script]`
- `promon reload [config|script]`
- `promon scale <config> <instances>`
- `promon status [name]`
- `promon logs [name] [-n lines] [--follow]`
- `promon watch [config|script]`
- `promon daemon start|stop|status|ping|list`
- `promon service install|start|stop|uninstall|status`
- `promon tui`

`promon daemon start` launches `promon daemon run <config>`, keeps desired apps reconciled, and exposes local IPC. On Unix platforms IPC uses a Unix socket under `PROMON_HOME/daemon`; on Windows it uses a localhost TCP listener address file.

When the daemon is reachable, normal management commands such as `start`, `stop`, `restart`, `reload`, `scale`, `status`, and `list` route through daemon IPC and update daemon desired state where applicable. If no daemon is reachable, they fall back to direct local process management.

For cluster apps, `scale` and `reload` use the cluster shim control channel when the app is already running, so the cluster master process can stay alive while workers are resized or replaced. Non-cluster apps still use the supervisor restart path.
