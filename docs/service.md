# Service Support

The current service implementation generates a user-level startup definition that runs:

```text
pumon daemon run <config>
```

Platform output:

- macOS: `~/Library/LaunchAgents/top.backrunner.pumon.plist`
- Linux: `~/.config/systemd/user/pumon.service`
- Windows: a command file under `PUMON_HOME/service`

On macOS the generated LaunchAgent writes stdout and stderr to `PUMON_HOME/daemon/service.out.log` and `PUMON_HOME/daemon/service.err.log`.

`pumon service start` and `pumon service stop` call `launchctl` on macOS and `systemctl --user` on Linux. `pumon service status` also reports backend-specific state such as loaded, active, and enabled where the platform supports it. Windows service registration is not implemented yet.
