#!/usr/bin/env bash
set -euo pipefail

cargo build -p promon-cli

PROMON_BIN="${PROMON_BIN:-target/debug/promon}"
"$PROMON_BIN" --version
"$PROMON_BIN" doctor
"$PROMON_BIN" validate examples/basic/ecosystem.config.json
"$PROMON_BIN" validate examples/typescript/ecosystem.config.ts
"$PROMON_BIN" validate examples/package-script/ecosystem.config.js
"$PROMON_BIN" validate fixtures/node-apps/ts-prebuilt/ecosystem.config.json
"$PROMON_BIN" validate fixtures/node-apps/package-script/ecosystem.config.js
"$PROMON_BIN" validate fixtures/node-apps/crash/ecosystem.config.json

tmp_home="$(mktemp -d /tmp/promon-smoke.XXXXXX)"
trap 'PROMON_HOME="$tmp_home" "$PROMON_BIN" stop basic-js >/dev/null 2>&1 || true; rm -rf "$tmp_home"' EXIT

PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/basic/ecosystem.config.json
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" list
PROMON_HOME="$tmp_home" "$PROMON_BIN" logs basic-js -n 5
PROMON_HOME="$tmp_home" "$PROMON_BIN" restart examples/basic/ecosystem.config.json
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop basic-js
PROMON_HOME="$tmp_home" "$PROMON_BIN" start examples/basic/server.js
sleep 1
PROMON_HOME="$tmp_home" "$PROMON_BIN" stop server

set +e
PROMON_HOME="$tmp_home" "$PROMON_BIN" start --wait fixtures/node-apps/crash/ecosystem.config.json
crash_code=$?
set -e
test "$crash_code" -ne 0
