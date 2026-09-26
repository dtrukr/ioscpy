#!/usr/bin/env node

const fs = require("fs");

function usage() {
  console.error("usage: scripts/validate-accessibility-selector-proof.js <schema.json> <proof.json> <tree.json> <device-udid> [action-result.json]");
}

const [, , schemaPath, proofPath, treePath, deviceUdid, actionResultPath] = process.argv;
if (!schemaPath || !proofPath || !treePath || !deviceUdid) {
  usage();
  process.exit(2);
}

const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const proof = JSON.parse(fs.readFileSync(proofPath, "utf8"));
const tree = JSON.parse(fs.readFileSync(treePath, "utf8"));
const actionResult = actionResultPath ? JSON.parse(fs.readFileSync(actionResultPath, "utf8")) : null;

function fail(message) {
  throw new Error(`${proofPath} does not match selector readiness proof: ${message}`);
}

if (proof.schema !== schema.properties.schema.const) fail("schema");
if (proof.device_udid !== deviceUdid) fail("device_udid");
if (typeof tree.snapshot_id !== "string" || tree.snapshot_id.length === 0) fail("tree.snapshot_id");
if (proof.tree_snapshot_id !== tree.snapshot_id) fail("tree_snapshot_id");
if (typeof proof.generated_at !== "string" || Number.isNaN(Date.parse(proof.generated_at))) fail("generated_at");
if (!Array.isArray(proof.cases) || proof.cases.length < 4) fail("cases");

const knownNodeIds = new Set((Array.isArray(tree.nodes) ? tree.nodes : []).map((node) => node && node.id).filter(Boolean));
const requiredTools = new Set(["find", "tap_selector", "form_input", "get_accessibility_tree"]);
const seenTools = new Set();
let sawTapAction = false;
let sawSetValueAction = false;

for (const [index, testCase] of proof.cases.entries()) {
  const path = `cases[${index}]`;
  if (!testCase || typeof testCase !== "object" || Array.isArray(testCase)) fail(path);
  if (testCase.ok !== true) fail(`${path}.ok`);
  if (!requiredTools.has(testCase.tool)) fail(`${path}.tool`);
  seenTools.add(testCase.tool);
  if (!testCase.selector || typeof testCase.selector !== "object" || Array.isArray(testCase.selector)) fail(`${path}.selector`);
  if (Object.keys(testCase.selector).length === 0) fail(`${path}.selector`);
  if (typeof testCase.matched_node_id !== "string" || testCase.matched_node_id.length === 0) fail(`${path}.matched_node_id`);
  if (!knownNodeIds.has(testCase.matched_node_id)) fail(`${path}.matched_node_id unknown`);
  if (testCase.tool === "tap_selector" && testCase.action === "tap") sawTapAction = true;
  if (testCase.tool === "form_input" && testCase.action === "set_value") sawSetValueAction = true;
}

for (const tool of requiredTools) {
  if (!seenTools.has(tool)) fail(`missing ${tool} case`);
}
if (!sawTapAction) fail("missing tap_selector tap action evidence");
if (!sawSetValueAction) fail("missing form_input set_value action evidence");
if (actionResult && actionResult.ok !== true) fail("action_result.ok");
if (actionResult && actionResult.action !== "tap") fail("action_result.action");

console.log("selector proof ok");
