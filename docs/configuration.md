# Configuration

Promon supports:

- `ecosystem.config.js`
- `ecosystem.config.cjs`
- `ecosystem.config.mjs`
- `ecosystem.config.ts`
- `ecosystem.config.mts`
- `ecosystem.config.cts`
- `ecosystem.config.json`
- `ecosystem.config.toml`
- `ecosystem.config.yaml`
- `ecosystem.config.yml`

Core fields: `name`, `script`, `command`, `cwd`, `args`, `node_args`, `interpreter`, `interpreter_args`, `package_manager`, `package_script`, `env`, `exec_mode`, `instances`, `watch`, `restart`, `max_memory_restart`, `cron_restart`, and `log`.

`watch` accepts either a boolean or an object. Object form supports `enabled`, `paths`, `include`, `ignore`, `debounce_ms`, and `reload`. `ignore_watch` is also accepted as a top-level PM2-style alias and is merged into `watch.ignore`.
