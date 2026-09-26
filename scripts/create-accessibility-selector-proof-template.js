#!/usr/bin/env node

const fs = require("fs");

function usage() {
  console.error("usage: scripts/create-accessibility-selector-proof-template.js <tree.json> <device-udid> [out.json]");
}

const [, , treePath, deviceUdid, outPath] = process.argv;
if (!treePath || !deviceUdid) {
  usage();
  process.exit(2);
}

const tree = JSON.parse(fs.readFileSync(treePath, "utf8"));
if (tree.schema !== "ioscpy.accessibility.v1") {
  throw new Error("tree.json must use schema ioscpy.accessibility.v1");
}
if (typeof tree.snapshot_id !== "string" || tree.snapshot_id.length === 0) {
  throw new Error("tree.json must include snapshot_id");
}

const nodes = Array.isArray(tree.nodes) ? tree.nodes : [];
if (nodes.length === 0) {
  throw new Error("tree.json has no nodes");
}

function visibleEnabled(node) {
  const frame = node && node.frame;
  return node
    && node.hidden !== true
    && node.enabled !== false
    && frame
    && Number(frame.width) > 1
    && Number(frame.height) > 1;
}

function nodeText(node) {
  return [
    node.label,
    node.value,
    node.hint,
    node.identifier,
    node.role,
    ...(Array.isArray(node.traits) ? node.traits : []),
  ].filter((part) => part !== undefined && part !== null && String(part).trim() !== "").join(" ");
}

function selectorForNode(node) {
  const selector = {};
  if (node.identifier) selector.identifier = String(node.identifier);
  if (node.label) selector.label = String(node.label);
  if (node.role) selector.role = String(node.role);
  if (Array.isArray(node.traits) && node.traits.length > 0) selector.traits = node.traits.map(String);
  if (Object.keys(selector).length === 0 && node.value !== undefined && node.value !== null) selector.value = node.value;
  if (Object.keys(selector).length === 0 && node.id) selector.id = String(node.id);
  return selector;
}

function roleIncludes(node, needles) {
  const haystack = nodeText(node).toLowerCase();
  return needles.some((needle) => haystack.includes(needle));
}

const candidates = nodes.filter(visibleEnabled);
const tappable = candidates.find((node) => roleIncludes(node, ["button", "link", "cell"])) ?? candidates[0] ?? nodes[0];
const editable = candidates.find((node) => roleIncludes(node, ["textfield", "text field", "searchfield", "search field", "editable"]))
  ?? candidates.find((node) => roleIncludes(node, ["text"]))
  ?? tappable;

const proof = {
  schema: "ioscpy.accessibility.selector-proof.v1",
  device_udid: deviceUdid,
  tree_snapshot_id: tree.snapshot_id,
  generated_at: new Date().toISOString(),
  notes: "Draft only. Exercise each case on the target device, replace ok=false with ok=true, and add evidence before using with --selector-ready.",
  cases: [
    {
      tool: "find",
      ok: false,
      selector: selectorForNode(tappable),
      matched_node_id: String(tappable.id),
      evidence: ["TODO: run MCP find with this selector and record the matching node id"],
    },
    {
      tool: "tap_selector",
      ok: false,
      selector: selectorForNode(tappable),
      matched_node_id: String(tappable.id),
      action: "tap",
      evidence: ["TODO: run MCP selector/ref tap and record visible device behavior"],
    },
    {
      tool: "form_input",
      ok: false,
      selector: selectorForNode(editable),
      matched_node_id: String(editable.id),
      action: "set_value",
      value_before: null,
      value_after: null,
      evidence: ["TODO: run MCP form_input on an editable node and record before/after values"],
    },
    {
      tool: "get_accessibility_tree",
      ok: false,
      selector: selectorForNode(tappable),
      matched_node_id: String(tappable.id),
      evidence: ["TODO: run MCP get_accessibility_tree and confirm this node appears with stable semantic fields"],
    },
  ],
};

const output = `${JSON.stringify(proof, null, 2)}\n`;
if (outPath) {
  fs.writeFileSync(outPath, output);
} else {
  process.stdout.write(output);
}
