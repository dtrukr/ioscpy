#!/usr/bin/env node

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const root = path.resolve(__dirname, "..");
const validator = path.join(root, "scripts", "validate-accessibility-summary.js");
const proofValidator = path.join(root, "scripts", "validate-accessibility-selector-proof.js");
const proofTemplateGenerator = path.join(root, "scripts", "create-accessibility-selector-proof-template.js");
const schema = path.join(root, "protocol", "accessibility-validation.schema.json");
const proofSchema = path.join(root, "protocol", "accessibility-selector-proof.schema.json");
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "ioscpy-ax-summary-test."));

function writeSummary(name, patch = {}) {
  const summary = {
    schema: "ioscpy.accessibility.validation.v1",
    ok: true,
    validated_at: "2026-01-01T00:00:00.000Z",
    tap_first: false,
    selector_ready: false,
    selector_proof: null,
    device_udid: "fixture-device",
    tree: {
      schema: "ioscpy.accessibility.v1",
      source: "fixture",
      snapshot_id: "fixture-snapshot",
      node_count: 1,
      truncated: false,
      host_application: null,
      screen: null,
    },
    action: null,
    artifacts: {
      devices: "devices.txt",
      debug: "debug.txt",
      tree: "tree.json",
      action_request: null,
      action_result: null,
      selector_proof: null,
    },
    ...patch,
  };
  const file = path.join(tmp, `${name}.json`);
  fs.writeFileSync(file, `${JSON.stringify(summary, null, 2)}\n`);
  return file;
}

function run(file) {
  return spawnSync(process.execPath, [validator, schema, file], {
    cwd: root,
    encoding: "utf8",
  });
}

function expectPass(name, file) {
  const result = run(file);
  if (result.status !== 0) {
    throw new Error(`${name} should pass\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  }
}

function expectFail(name, file) {
  const result = run(file);
  if (result.status === 0) {
    throw new Error(`${name} should fail`);
  }
}

const treeFile = path.join(tmp, "tree.json");
fs.writeFileSync(treeFile, `${JSON.stringify({
  schema: "ioscpy.accessibility.v1",
  source: "fixture",
  snapshot_id: "fixture-snapshot",
  nodes: [
    { id: "n0", role: "button", frame: { x: 0, y: 0, width: 10, height: 10 }, children: [] },
    { id: "n1", role: "textField", frame: { x: 0, y: 20, width: 100, height: 20 }, children: [] },
  ],
})}\n`);
const actionResultFile = path.join(tmp, "action-result.json");
fs.writeFileSync(actionResultFile, `${JSON.stringify({
  schema: "ioscpy.accessibility.action-result.v1",
  ok: true,
  action: "tap",
  target_id: "n0",
})}\n`);
const proofFile = path.join(tmp, "selector-proof.json");
fs.writeFileSync(proofFile, `${JSON.stringify({
  schema: "ioscpy.accessibility.selector-proof.v1",
  device_udid: "fixture-device",
  tree_snapshot_id: "fixture-snapshot",
  generated_at: "2026-01-01T00:00:00.000Z",
  cases: [
    { tool: "find", ok: true, selector: { label: "Continue" }, matched_node_id: "n0" },
    { tool: "tap_selector", ok: true, selector: { label: "Continue", role: "button" }, matched_node_id: "n0", action: "tap" },
    { tool: "form_input", ok: true, selector: { identifier: "email" }, matched_node_id: "n1", action: "set_value", value_before: "", value_after: "user@example.com" },
    { tool: "get_accessibility_tree", ok: true, selector: { id: "n0" }, matched_node_id: "n0" },
  ],
})}\n`);

{
  const result = spawnSync(process.execPath, [proofValidator, proofSchema, proofFile, treeFile, "fixture-device", actionResultFile], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`selector proof should pass\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  }
}
const incompleteProofFile = path.join(tmp, "selector-proof-incomplete.json");
fs.writeFileSync(incompleteProofFile, `${JSON.stringify({
  schema: "ioscpy.accessibility.selector-proof.v1",
  device_udid: "fixture-device",
  tree_snapshot_id: "fixture-snapshot",
  generated_at: "2026-01-01T00:00:00.000Z",
  cases: [
    { tool: "find", ok: true, selector: { label: "Continue" }, matched_node_id: "n0" },
  ],
})}\n`);
{
  const result = spawnSync(process.execPath, [proofValidator, proofSchema, incompleteProofFile, treeFile, "fixture-device", actionResultFile], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status === 0) {
    throw new Error("incomplete selector proof should fail");
  }
}
const proofTemplateFile = path.join(tmp, "selector-proof-template.json");
{
  const result = spawnSync(process.execPath, [proofTemplateGenerator, treeFile, "fixture-device", proofTemplateFile], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`selector proof template should generate\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  }
  const draft = JSON.parse(fs.readFileSync(proofTemplateFile, "utf8"));
  if (draft.schema !== "ioscpy.accessibility.selector-proof.v1") {
    throw new Error("selector proof template has wrong schema");
  }
  if (!Array.isArray(draft.cases) || draft.cases.length !== 4) {
    throw new Error("selector proof template should contain four cases");
  }
  if (draft.cases.some((testCase) => testCase.ok !== false)) {
    throw new Error("selector proof template cases must start as ok=false");
  }
}
{
  const result = spawnSync(process.execPath, [proofValidator, proofSchema, proofTemplateFile, treeFile, "fixture-device", actionResultFile], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status === 0) {
    throw new Error("selector proof template should not pass readiness validation before real evidence is filled in");
  }
}

const prototypeOnly = writeSummary("prototype-only");
expectPass("prototype-only summary", prototypeOnly);

const selectorReady = writeSummary("selector-ready", {
  tap_first: true,
  selector_ready: true,
  selector_proof: {
    file: "selector-proof.json",
    schema: "ioscpy.accessibility.selector-proof.v1",
    sha256: "0".repeat(64),
    bytes: 1,
  },
  action: {
    schema: "ioscpy.accessibility.action-result.v1",
    ok: true,
    action: "tap",
    target_id: "n0",
    point: { x: 0.5, y: 0.5, coordinate_space: "normalized" },
  },
  artifacts: {
    devices: "devices.txt",
    debug: "debug.txt",
    tree: "tree.json",
    action_request: "action-request.json",
    action_result: "action-result.json",
    selector_proof: "selector-proof.json",
  },
});
expectPass("selector-ready summary", selectorReady);

const missingProof = writeSummary("missing-proof", {
  tap_first: true,
  selector_ready: true,
  action: {
    schema: "ioscpy.accessibility.action-result.v1",
    ok: true,
    action: "tap",
  },
});
expectFail("selector-ready without proof", missingProof);

const missingAction = writeSummary("missing-action", {
  tap_first: true,
  selector_ready: true,
  selector_proof: {
    file: "selector-proof.json",
    schema: "ioscpy.accessibility.selector-proof.v1",
    sha256: "1".repeat(64),
    bytes: 1,
  },
});
expectFail("selector-ready without action", missingAction);

const missingDevice = writeSummary("missing-device", {
  tap_first: true,
  selector_ready: true,
  device_udid: null,
  selector_proof: {
    file: "selector-proof.json",
    schema: "ioscpy.accessibility.selector-proof.v1",
    sha256: "2".repeat(64),
    bytes: 1,
  },
  action: {
    schema: "ioscpy.accessibility.action-result.v1",
    ok: true,
    action: "tap",
  },
});
expectFail("selector-ready without device_udid", missingDevice);

const missingProofSchema = writeSummary("missing-proof-schema", {
  tap_first: true,
  selector_ready: true,
  selector_proof: {
    file: "selector-proof.json",
    sha256: "3".repeat(64),
    bytes: 1,
  },
  action: {
    schema: "ioscpy.accessibility.action-result.v1",
    ok: true,
    action: "tap",
  },
});
expectFail("selector-ready without proof schema", missingProofSchema);

console.log("accessibility validation summary fixtures ok");
