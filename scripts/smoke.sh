#!/usr/bin/env bash
set -euo pipefail

cargo build -p promon-cli

PROMON_BIN="${PROMON_BIN:-target/debug/promon}"
"$PROMON_BIN" --version
"$PROMON_BIN" doctor
"$PROMON_BIN" validate examples/basic/ecosystem.config.json
"$PROMON_BIN" validate examples/typescript/ecosystem.config.ts
"$PROMON_BIN" validate examples/package-script/ecosystem.config.js
"$PROMON_BIN" validate examples/cluster/ecosystem.config.json
"$PROMON_BIN" validate fixtures/node-apps/ts-prebuilt/ecosystem.config.json
"$PROMON_BIN" validate fixtures/node-apps/package-script/ecosystem.config.js
"$PROMON_BIN" validate fixtures/node-apps/crash/ecosystem.config.json
"$PROMON_BIN" validate fixtures/node-apps/scheduled/ecosystem.config.json
"$PROMON_BIN" service status

tmp_home="$(mktemp -d /tmp/promon-smoke.XXXXXX)"
trap 'PROMON_HOME="$tmp_home" "$PROMON_BIN" stop basic-js >/dev/null 2>&1 || true; rm -rf "$tmp_home"' EXIT

PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/basic/ecosystem.config.json
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" list
PROMON_HOME="$tmp_home" "$PROMON_BIN" status basic-js
status_json="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json status basic-js)"
node -e 'const r = JSON.parse(process.argv[1]); if (r.count !== 1 || r.processes[0].name !== "basic-js") process.exit(1);' "$status_json"
PROMON_HOME="$tmp_home" "$PROMON_BIN" logs basic-js -n 5
PROMON_HOME="$tmp_home" "$PROMON_BIN" reload examples/basic/ecosystem.config.json
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" restart examples/basic/ecosystem.config.json
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop basic-js
PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/basic/server.js
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop server
PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/cluster/ecosystem.config.json
sleep 1
cluster_before="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
PROMON_HOME="$tmp_home" "$PROMON_BIN" scale examples/cluster/ecosystem.config.json 1
sleep 1
cluster_after_scale="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
node -e 'const before = JSON.parse(process.argv[1]).processes.find((p) => p.name === "cluster-js"); const after = JSON.parse(process.argv[2]).processes.find((p) => p.name === "cluster-js"); if (!before || !after || before.pid !== after.pid) process.exit(1);' "$cluster_before" "$cluster_after_scale"
PROMON_HOME="$tmp_home" "$PROMON_BIN" reload examples/cluster/ecosystem.config.json
sleep 1
cluster_after_reload="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
node -e 'const before = JSON.parse(process.argv[1]).processes.find((p) => p.name === "cluster-js"); const after = JSON.parse(process.argv[2]).processes.find((p) => p.name === "cluster-js"); if (!before || !after || before.pid !== after.pid) process.exit(1);' "$cluster_after_scale" "$cluster_after_reload"
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop cluster-js
HOME="$tmp_home" "$PROMON_BIN" service install examples/basic/ecosystem.config.json
HOME="$tmp_home" "$PROMON_BIN" service status
HOME="$tmp_home" "$PROMON_BIN" service uninstall
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon start examples/basic/ecosystem.config.json
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon status
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon ping
ping_json="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json daemon ping)"
node -e 'const r = JSON.parse(process.argv[1]); if (r.version !== 1 || !r.request_id || !r.ok || r.payload.pong !== true) process.exit(1);' "$ping_json"
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon list
PROMON_HOME="$tmp_home" "$PROMON_BIN" list
PROMON_HOME="$tmp_home" "$PROMON_BIN" status basic-js
daemon_status_json="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json status basic-js)"
node -e 'const r = JSON.parse(process.argv[1]); if (r.count !== 1 || r.processes[0].name !== "basic-js") process.exit(1);' "$daemon_status_json"
PROMON_HOME="$tmp_home" "$PROMON_BIN" reload examples/basic/ecosystem.config.json
PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/cluster/ecosystem.config.json
sleep 1
daemon_cluster_before="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
PROMON_HOME="$tmp_home" "$PROMON_BIN" scale examples/cluster/ecosystem.config.json 1
sleep 1
daemon_cluster_after_scale="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
node -e 'const before = JSON.parse(process.argv[1]).payload.processes.find((p) => p.name === "cluster-js"); const after = JSON.parse(process.argv[2]).payload.processes.find((p) => p.name === "cluster-js"); if (!before || !after || before.pid !== after.pid) process.exit(1);' "$daemon_cluster_before" "$daemon_cluster_after_scale"
PROMON_HOME="$tmp_home" "$PROMON_BIN" reload examples/cluster/ecosystem.config.json
sleep 1
daemon_cluster_after_reload="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
node -e 'const before = JSON.parse(process.argv[1]).payload.processes.find((p) => p.name === "cluster-js"); const after = JSON.parse(process.argv[2]).payload.processes.find((p) => p.name === "cluster-js"); if (!before || !after || before.pid !== after.pid) process.exit(1);' "$daemon_cluster_after_scale" "$daemon_cluster_after_reload"
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop cluster-js
PROMON_HOME="$tmp_home" "$PROMON_BIN" restart examples/basic/ecosystem.config.json
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop basic-js
PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/basic/ecosystem.config.json
PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/package-script/ecosystem.config.js
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon stop
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon start examples/basic/ecosystem.config.json
sleep 1
restored_json="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
node -e 'const r = JSON.parse(process.argv[1]); const names = new Set(r.payload.processes.map((p) => p.name)); if (!names.has("basic-js") || !names.has("package-script")) process.exit(1);' "$restored_json"
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop all
PROMON_HOME="$tmp_home" "$PROMON_BIN" start fixtures/node-apps/scheduled/ecosystem.config.json
scheduled_first="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
sleep 4
scheduled_second="$(PROMON_HOME="$tmp_home" "$PROMON_BIN" --json list)"
node -e 'const first = JSON.parse(process.argv[1]).payload.processes.find((p) => p.name === "scheduled-fixture"); const second = JSON.parse(process.argv[2]).payload.processes.find((p) => p.name === "scheduled-fixture"); if (!first || !second || first.pid === second.pid) process.exit(1);' "$scheduled_first" "$scheduled_second"
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop all
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon stop
PROMON_HOME="$tmp_home" "$PROMON_BIN" daemon stop
PROMON_HOME="$tmp_home" "$PROMON_BIN" list

set +e
PROMON_HOME="$tmp_home" "$PROMON_BIN" start --wait fixtures/node-apps/crash/ecosystem.config.json
crash_code=$?
set -e
test "$crash_code" -ne 0
