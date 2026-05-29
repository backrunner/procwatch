#!/usr/bin/env node
import path from "node:path";

const binary = process.platform === "win32" ? "pumon.exe" : "pumon";
console.log(path.resolve("target", "debug", binary));

