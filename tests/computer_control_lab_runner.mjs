#!/usr/bin/env node

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = dirname(fileURLToPath(import.meta.url));
const htmlPath = join(root, "computer_control_lab.html");
const manifestPath = join(root, "computer_control_scenarios.json");

function parseArgs(argv) {
  const options = {
    serve: false,
    json: false,
    list: false,
    port: 0,
    scenario: null,
    report: null,
    toolActions: null
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--serve") options.serve = true;
    else if (arg === "--json") options.json = true;
    else if (arg === "--list") options.list = true;
    else if (arg === "--port") options.port = Number(argv[++index]);
    else if (arg === "--scenario") options.scenario = argv[++index];
    else if (arg === "--report") options.report = argv[++index];
    else if (arg === "--tool-actions") options.toolActions = Number(argv[++index]);
    else if (arg === "--help" || arg === "-h") return { help: true };
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!Number.isInteger(options.port) || options.port < 0 || options.port > 65535) {
    throw new Error("--port must be an integer from 0 through 65535");
  }
  if (options.toolActions !== null &&
      (!Number.isInteger(options.toolActions) || options.toolActions < 0)) {
    throw new Error("--tool-actions must be a non-negative integer");
  }
  return options;
}

function help() {
  return `Computer Control capability-lab validator and local host

Usage:
  node tests/computer_control_lab_runner.mjs
  node tests/computer_control_lab_runner.mjs --list [--json]
  node tests/computer_control_lab_runner.mjs --serve [--scenario ID] [--port PORT]
  node tests/computer_control_lab_runner.mjs --scenario ID --report report.json [--tool-actions N]

The default command validates the manifest/fixture contract. --serve binds only
to 127.0.0.1 and prints deterministic scenario URLs plus the task and action
budget. A browser-side report comes from window.capabilityLab.getReport().`;
}

function parseHtmlTasks(html) {
  const tasks = new Map();
  const pattern = /^      ([a-z][a-z0-9_]*): \{\r?\n        task: ("(?:[^"\\]|\\.)*"),/gm;
  for (const match of html.matchAll(pattern)) {
    tasks.set(match[1], JSON.parse(match[2]));
  }
  return tasks;
}

function validateFixture(html, manifest) {
  const errors = [];
  if (!Array.isArray(manifest) || manifest.length === 0) {
    return ["scenario manifest must be a non-empty array"];
  }
  const ids = new Set();
  for (const [index, scenario] of manifest.entries()) {
    const at = `scenario[${index}]`;
    if (!scenario || typeof scenario !== "object") {
      errors.push(`${at} must be an object`);
      continue;
    }
    if (typeof scenario.id !== "string" || !/^[a-z][a-z0-9_]*$/.test(scenario.id)) {
      errors.push(`${at}.id must be a stable snake_case identifier`);
    } else if (ids.has(scenario.id)) {
      errors.push(`duplicate scenario id: ${scenario.id}`);
    } else {
      ids.add(scenario.id);
    }
    if (typeof scenario.task !== "string" || scenario.task.trim().length < 12) {
      errors.push(`${at}.task must be a concrete non-empty instruction`);
    }
    if (!Array.isArray(scenario.capabilities) || scenario.capabilities.length === 0 ||
        scenario.capabilities.some(value => typeof value !== "string" || !value.trim())) {
      errors.push(`${at}.capabilities must contain non-empty strings`);
    } else if (new Set(scenario.capabilities).size !== scenario.capabilities.length) {
      errors.push(`${at}.capabilities contains duplicates`);
    }
    if (scenario.expected_title_suffix !== "PASS") {
      errors.push(`${at}.expected_title_suffix must be PASS`);
    }
    if (!Number.isInteger(scenario.max_actions) || scenario.max_actions < 1) {
      errors.push(`${at}.max_actions must be a positive integer`);
    }
  }

  const htmlTasks = parseHtmlTasks(html);
  for (const scenario of manifest) {
    if (!scenario?.id) continue;
    if (!htmlTasks.has(scenario.id)) {
      errors.push(`manifest scenario ${scenario.id} has no HTML implementation`);
    } else if (htmlTasks.get(scenario.id) !== scenario.task) {
      errors.push(`task drift for ${scenario.id}: manifest and HTML differ`);
    }
  }
  for (const id of htmlTasks.keys()) {
    if (!ids.has(id)) errors.push(`HTML scenario ${id} is absent from the manifest`);
  }

  const requiredContracts = [
    ["window.capabilityLab", "browser-side report API"],
    ["getReport:", "machine-readable result"],
    ["submitObservationAnswer:", "read-only answer oracle"],
    ["clearTimeout(delayTimer)", "delayed-operation reset"],
    ["dialog.open", "modal reset"],
    ["frame.srcdoc = frameSource", "embedded-surface reset"],
    ["forbidden_interaction", "read-only interaction audit"]
  ];
  for (const [needle, label] of requiredContracts) {
    if (!html.includes(needle)) errors.push(`fixture is missing ${label}`);
  }
  const observe = manifest.find(item => item.id === "observe_only");
  if (!observe || typeof observe.expected_answer !== "string") {
    errors.push("observe_only must declare expected_answer for transcript validation");
  }
  return errors;
}

function validateReport(report, scenario, toolActions) {
  const errors = [];
  if (!report || typeof report !== "object") return ["report must be a JSON object"];
  if (report.schema_version !== 1) errors.push("report schema_version must be 1");
  if (report.scenario_id !== scenario.id) {
    errors.push(`report scenario ${report.scenario_id ?? "(missing)"} does not match ${scenario.id}`);
  }
  if (report.passed !== true) errors.push("fixture oracle did not report passed=true");
  if (!Array.isArray(report.events)) errors.push("report.events must be an array");
  else {
    for (let index = 0; index < report.events.length; index += 1) {
      if (report.events[index]?.sequence !== index + 1) {
        errors.push(`report event sequence is not contiguous at index ${index}`);
        break;
      }
    }
  }
  if (scenario.id === "observe_only") {
    if (report.forbidden_interactions !== 0) {
      errors.push("observe_only recorded a trusted pointer, keyboard, focus, or scroll interaction");
    }
    if (String(report.submitted_observation ?? "").trim() !== scenario.expected_answer) {
      errors.push("observe_only answer does not match expected_answer");
    }
  }
  if (toolActions !== null && toolActions > scenario.max_actions) {
    errors.push(`tool action budget exceeded: ${toolActions} > ${scenario.max_actions}`);
  }
  return errors;
}

function send(response, status, contentType, body) {
  response.writeHead(status, {
    "content-type": contentType,
    "cache-control": "no-store",
    "x-content-type-options": "nosniff"
  });
  response.end(body);
}

async function serve(html, manifest, selectedId, port) {
  const selected = selectedId ? manifest.find(item => item.id === selectedId) : null;
  if (selectedId && !selected) throw new Error(`unknown scenario: ${selectedId}`);
  const manifestJson = `${JSON.stringify(manifest, null, 2)}\n`;
  const server = createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    if (url.pathname === "/health") return send(response, 200, "application/json", '{"ok":true}\n');
    if (url.pathname === "/api/scenarios" || url.pathname === "/computer_control_scenarios.json") {
      return send(response, 200, "application/json; charset=utf-8", manifestJson);
    }
    if (url.pathname === "/") {
      const id = selected?.id ?? "manual";
      response.writeHead(302, { location: `/computer_control_lab.html?scenario=${encodeURIComponent(id)}` });
      return response.end();
    }
    if (url.pathname === "/computer_control_lab.html") {
      return send(response, 200, "text/html; charset=utf-8", html);
    }
    return send(response, 404, "text/plain; charset=utf-8", "Not found\n");
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  const address = server.address();
  const actualPort = typeof address === "object" && address ? address.port : port;
  const base = `http://127.0.0.1:${actualPort}/computer_control_lab.html`;
  if (selected) {
    console.log(`URL=${base}?scenario=${encodeURIComponent(selected.id)}`);
    console.log(`SCENARIO=${selected.id}`);
    console.log(`MAX_ACTIONS=${selected.max_actions}`);
    console.log(`TASK=${selected.task}`);
  } else {
    console.log(`INDEX=http://127.0.0.1:${actualPort}/`);
    for (const scenario of manifest) {
      console.log(`${scenario.id}\t${base}?scenario=${encodeURIComponent(scenario.id)}`);
    }
  }
  console.log("Press Ctrl+C to stop the local lab server.");
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) return console.log(help());
  const [html, manifestText] = await Promise.all([
    readFile(htmlPath, "utf8"),
    readFile(manifestPath, "utf8")
  ]);
  const manifest = JSON.parse(manifestText);
  const fixtureErrors = validateFixture(html, manifest);
  if (fixtureErrors.length) throw new Error(`fixture validation failed:\n- ${fixtureErrors.join("\n- ")}`);
  const selected = options.scenario
    ? manifest.find(item => item.id === options.scenario)
    : null;
  if (options.scenario && !selected) throw new Error(`unknown scenario: ${options.scenario}`);

  if (options.report) {
    if (!selected) throw new Error("--report requires --scenario");
    const report = JSON.parse(await readFile(options.report, "utf8"));
    const reportErrors = validateReport(report, selected, options.toolActions);
    if (reportErrors.length) throw new Error(`report validation failed:\n- ${reportErrors.join("\n- ")}`);
    console.log(`PASS ${selected.id}`);
    return;
  }
  if (options.serve) return serve(html, manifest, options.scenario, options.port);
  const output = selected ? [selected] : manifest;
  if (options.json) console.log(JSON.stringify(output, null, 2));
  else if (options.list || selected) {
    for (const scenario of output) {
      console.log(`${scenario.id}\tmax_actions=${scenario.max_actions}\t${scenario.task}`);
    }
  } else {
    console.log(`PASS fixture contract (${manifest.length} scenarios)`);
  }
}

main().catch(error => {
  console.error(`ERROR: ${error.message}`);
  process.exitCode = 1;
});
