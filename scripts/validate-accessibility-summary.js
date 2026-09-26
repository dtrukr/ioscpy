#!/usr/bin/env node

const fs = require("fs");

function usage() {
  console.error("usage: scripts/validate-accessibility-summary.js <schema.json> <validation-summary.json>");
}

const [, , schemaPath, summaryPath] = process.argv;
if (!schemaPath || !summaryPath) {
  usage();
  process.exit(2);
}

const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const summary = JSON.parse(fs.readFileSync(summaryPath, "utf8"));

function fail(message) {
  throw new Error(`validation-summary.json does not match accessibility-validation schema: ${message}`);
}

if (summary.schema !== schema.properties.schema.const) fail("schema");
if (summary.ok !== true) fail("ok");
if (typeof summary.validated_at !== "string" || Number.isNaN(Date.parse(summary.validated_at))) fail("validated_at");
if (typeof summary.tap_first !== "boolean") fail("tap_first");
if (typeof summary.selector_ready !== "boolean") fail("selector_ready");
if (summary.selector_ready && summary.tap_first !== true) fail("selector_ready requires tap_first");
if (summary.selector_ready && !summary.action) fail("selector_ready requires action");
if (summary.selector_ready && !summary.selector_proof) fail("selector_ready requires selector_proof");
if (summary.selector_ready && (typeof summary.device_udid !== "string" || summary.device_udid.length === 0)) fail("selector_ready requires device_udid");
if (summary.selector_proof !== null && summary.selector_proof !== undefined) {
  if (!summary.selector_proof || typeof summary.selector_proof !== "object") fail("selector_proof");
  if (typeof summary.selector_proof.file !== "string" || summary.selector_proof.file.length === 0) fail("selector_proof.file");
  if (summary.selector_proof.schema !== "ioscpy.accessibility.selector-proof.v1") fail("selector_proof.schema");
  if (typeof summary.selector_proof.sha256 !== "string" || !/^[0-9a-f]{64}$/.test(summary.selector_proof.sha256)) fail("selector_proof.sha256");
  if (!Number.isInteger(summary.selector_proof.bytes) || summary.selector_proof.bytes < 1) fail("selector_proof.bytes");
}
if (summary.device_udid !== null && summary.device_udid !== undefined && typeof summary.device_udid !== "string") fail("device_udid");
if (!summary.tree || typeof summary.tree !== "object") fail("tree");
if (summary.tree.schema !== schema.properties.tree.properties.schema.const) fail("tree.schema");
if (typeof summary.tree.source !== "string" || summary.tree.source.length === 0) fail("tree.source");
if (!Number.isInteger(summary.tree.node_count) || summary.tree.node_count < 1) fail("tree.node_count");
if (typeof summary.tree.truncated !== "boolean") fail("tree.truncated");
if (summary.action !== null && summary.action !== undefined) {
  if (!summary.action || typeof summary.action !== "object") fail("action");
  if (summary.action.schema !== schema.properties.action.properties.schema.const) fail("action.schema");
  if (summary.action.ok !== true) fail("action.ok");
}
if (!summary.artifacts || typeof summary.artifacts !== "object") fail("artifacts");
for (const key of ["devices", "debug", "tree"]) {
  if (typeof summary.artifacts[key] !== "string" || summary.artifacts[key].length === 0) fail(`artifacts.${key}`);
}

console.log("validation summary ok");
