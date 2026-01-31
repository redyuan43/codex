#!/usr/bin/env node
"use strict";

const http = require("http");
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const HELPER_PORT = 7100;
const BRIDGE_HOST = "127.0.0.1";
const BRIDGE_PORT = 7000;
const CODEX_RS_DIR = path.resolve(__dirname, "..", "..");
const INDEX_HTML = path.join(__dirname, "index.html");

let bridgeProc = null;

function writeJson(res, status, obj) {
  const body = JSON.stringify(obj);
  res.writeHead(status, {
    "Content-Type": "application/json",
    "Content-Length": Buffer.byteLength(body),
  });
  res.end(body);
}

function startBridge(res) {
  if (bridgeProc && bridgeProc.exitCode == null) {
    return writeJson(res, 200, { running: true, pid: bridgeProc.pid });
  }

  bridgeProc = spawn(
    "cargo",
    [
      "run",
      "-p",
      "codex-app-server-test-client",
      "--bin",
      "http_bridge",
      "--",
      "--listen",
      `${BRIDGE_HOST}:${BRIDGE_PORT}`,
    ],
    {
      cwd: CODEX_RS_DIR,
      stdio: ["ignore", "pipe", "pipe"],
      env: { ...process.env, RUST_LOG: "info" },
    }
  );

  bridgeProc.stdout.on("data", (chunk) => {
    process.stdout.write(`[http_bridge] ${chunk}`);
  });
  bridgeProc.stderr.on("data", (chunk) => {
    process.stderr.write(`[http_bridge] ${chunk}`);
  });
  bridgeProc.on("exit", (code) => {
    process.stderr.write(`[http_bridge] exited with code ${code}\n`);
  });

  writeJson(res, 200, { running: true, pid: bridgeProc.pid });
}

function stopBridge(res) {
  if (!bridgeProc || bridgeProc.exitCode != null) {
    return writeJson(res, 200, { running: false });
  }
  bridgeProc.kill("SIGTERM");
  writeJson(res, 200, { running: false });
}

function status(res) {
  const running = bridgeProc && bridgeProc.exitCode == null;
  writeJson(res, 200, { running, pid: running ? bridgeProc.pid : null });
}

function proxy(req, res) {
  const targetPath = req.url.replace(/^\/api/, "") || "/";
  const options = {
    hostname: BRIDGE_HOST,
    port: BRIDGE_PORT,
    method: req.method,
    path: targetPath,
    headers: { ...req.headers, host: `${BRIDGE_HOST}:${BRIDGE_PORT}` },
  };

  const proxyReq = http.request(options, (proxyRes) => {
    res.writeHead(proxyRes.statusCode || 500, proxyRes.headers);
    proxyRes.pipe(res);
  });

  proxyReq.on("error", (err) => {
    writeJson(res, 502, { error: err.message });
  });

  req.pipe(proxyReq);
}

const server = http.createServer((req, res) => {
  if (req.url === "/" || req.url === "/index.html") {
    const html = fs.readFileSync(INDEX_HTML, "utf8");
    res.writeHead(200, { "Content-Type": "text/html" });
    res.end(html);
    return;
  }

  if (req.url === "/bridge/start" && req.method === "POST") {
    return startBridge(res);
  }
  if (req.url === "/bridge/stop" && req.method === "POST") {
    return stopBridge(res);
  }
  if (req.url === "/bridge/status") {
    return status(res);
  }

  if (req.url.startsWith("/api/")) {
    return proxy(req, res);
  }

  res.writeHead(404, { "Content-Type": "text/plain" });
  res.end("not found");
});

server.listen(HELPER_PORT, "127.0.0.1", () => {
  process.stdout.write(`helper listening at http://127.0.0.1:${HELPER_PORT}\n`);
});
