#!/usr/bin/env node

const fs = require("fs");

function usage() {
  console.error("usage: scripts/validate-accessibility-artifact.js <tree|action-request|action-result> <schema.json> <artifact.json>");
}

const [, , kind, schemaPath, artifactPath] = process.argv;
if (!kind || !schemaPath || !artifactPath) {
  usage();
  process.exit(2);
}

const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const artifact = JSON.parse(fs.readFileSync(artifactPath, "utf8"));

function fail(message) {
  throw new Error(`${artifactPath} does not match ${kind} schema: ${message}`);
}

function isFiniteNumber(value) {
  return typeof value === "number" && Number.isFinite(value);
}

function validateRect(rect, path) {
  if (!rect || typeof rect !== "object" || Array.isArray(rect)) fail(path);
  for (const key of ["x", "y", "width", "height"]) {
    if (!isFiniteNumber(rect[key])) fail(`${path}.${key}`);
  }
}

function validatePoint(point, path) {
  if (!point || typeof point !== "object" || Array.isArray(point)) fail(path);
  if (!isFiniteNumber(point.x)) fail(`${path}.x`);
  if (!isFiniteNumber(point.y)) fail(`${path}.y`);
}

function validateTree() {
  const treeSchema = schema.properties;
  if (artifact.schema !== treeSchema.schema.const) fail("schema");
  if (typeof artifact.source !== "string" || artifact.source.length === 0) fail("source");
  if (typeof artifact.snapshot_id !== "string" || artifact.snapshot_id.length === 0) fail("snapshot_id");
  if (!Array.isArray(artifact.nodes) || artifact.nodes.length < 1) fail("nodes");
  if (artifact.truncated !== undefined && typeof artifact.truncated !== "boolean") fail("truncated");
  for (const [index, node] of artifact.nodes.entries()) {
    const path = `nodes[${index}]`;
    if (!node || typeof node !== "object" || Array.isArray(node)) fail(path);
    if (typeof node.id !== "string" || node.id.length === 0) fail(`${path}.id`);
    if (typeof node.role !== "string" || node.role.length === 0) fail(`${path}.role`);
    validateRect(node.frame, `${path}.frame`);
    if (!Array.isArray(node.children)) fail(`${path}.children`);
  }
  console.log(`tree ok: ${artifact.nodes.length} nodes from ${artifact.source}`);
}

function validateActionRequest() {
  const requestSchema = schema.$defs.request.properties;
  if (artifact.schema !== requestSchema.schema.const) fail("schema");
  if (typeof artifact.action !== "string") fail("action");
  if (!schema.$defs.request.properties.action.enum.includes(artifact.action)) fail("action enum");
  if (artifact.point !== undefined) validatePoint(artifact.point, "point");
  if (artifact.frame !== undefined) validateRect(artifact.frame, "frame");
  if (artifact.node !== undefined && artifact.node !== null) {
    if (typeof artifact.node !== "object" || Array.isArray(artifact.node)) fail("node");
    if (artifact.node.frame !== undefined) validateRect(artifact.node.frame, "node.frame");
  }
  console.log(`action request ok: ${artifact.action}`);
}

function validateActionResult() {
  const resultSchema = schema.$defs.result.properties;
  if (artifact.schema !== resultSchema.schema.const) fail("schema");
  if (typeof artifact.ok !== "boolean") fail("ok");
  if (typeof artifact.action !== "string") fail("action");
  if (artifact.point !== undefined && artifact.point !== null) validatePoint(artifact.point, "point");
  if (artifact.ok !== true) fail(`action failed: ${JSON.stringify(artifact)}`);
  console.log(`action result ok: ${artifact.action}`);
}

switch (kind) {
  case "tree":
    validateTree();
    break;
  case "action-request":
    validateActionRequest();
    break;
  case "action-result":
    validateActionResult();
    break;
  default:
    usage();
    process.exit(2);
}
