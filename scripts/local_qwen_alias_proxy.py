#!/usr/bin/env python3
"""Expose local Qwen presets behind Codex-friendly model names."""

from __future__ import annotations

import argparse
import copy
import http.client
import http.server
import json
import socketserver
import sys
import urllib.parse
from typing import Any


INTERNAL_PRESETS: dict[str, dict[str, Any]] = {
    "qwen36-think-general": {
        "description": "Thinking mode, general preset.",
        "temperature": 1.0,
        "top_p": 0.95,
        "top_k": 20,
        "min_p": 0,
        "presence_penalty": 1.5,
        "reasoning_format": "deepseek",
        "enable_thinking": True,
    },
    "qwen36-think-code": {
        "description": "Thinking mode, coding and precise preset.",
        "temperature": 0.6,
        "top_p": 0.95,
        "top_k": 20,
        "min_p": 0,
        "presence_penalty": 0.0,
        "reasoning_format": "deepseek",
        "enable_thinking": True,
    },
    "qwen36-nothink-general": {
        "description": "Non-thinking mode, general preset.",
        "temperature": 0.7,
        "top_p": 0.8,
        "top_k": 20,
        "min_p": 0,
        "presence_penalty": 1.5,
        "reasoning_format": "none",
        "enable_thinking": False,
    },
    "qwen36-nothink-reason": {
        "description": "Non-thinking mode, reasoning preset.",
        "temperature": 1.0,
        "top_p": 1.0,
        "top_k": 40,
        "min_p": 0,
        "presence_penalty": 2.0,
        "reasoning_format": "none",
        "enable_thinking": False,
    },
}

THINKING_EFFORTS = [
    {
        "effort": "low",
        "description": "Fast responses with lighter reasoning",
    },
    {
        "effort": "medium",
        "description": "Balances speed and reasoning depth for everyday tasks",
    },
    {
        "effort": "high",
        "description": "Greater reasoning depth for complex problems",
    },
    {
        "effort": "xhigh",
        "description": "Extra high reasoning depth for complex problems",
    },
]

REASONING_EFFORTS = [
    {
        "effort": "low",
        "description": "Balances speed with some reasoning for straightforward tasks",
    },
    {
        "effort": "medium",
        "description": "Provides a solid balance of reasoning depth and latency",
    },
    {
        "effort": "high",
        "description": "Maximizes reasoning depth for complex or ambiguous problems",
    },
    {
        "effort": "xhigh",
        "description": "Extra high reasoning for complex problems",
    },
]

MINI_EFFORTS = [
    {
        "effort": "medium",
        "description": "Dynamically adjusts reasoning based on the task",
    },
    {
        "effort": "high",
        "description": "Maximizes reasoning depth for complex or ambiguous problems",
    },
]

DISPLAY_MODELS: dict[str, dict[str, Any]] = {
    "gpt-5.3-codex": {
        "backend_alias": "qwen36-think-code",
        "description": "Mapped to the local Qwen thinking preset for coding and precise tasks.",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": THINKING_EFFORTS,
        "priority": 4,
    },
    "gpt-5.4": {
        "backend_alias": "qwen36-think-general",
        "description": "Mapped to the local Qwen thinking preset for general work.",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": THINKING_EFFORTS,
        "priority": 3,
    },
    "gpt-5.2": {
        "backend_alias": "qwen36-nothink-reason",
        "description": "Mapped to the local Qwen non-thinking preset tuned for reasoning tasks.",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": REASONING_EFFORTS,
        "priority": 2,
    },
    "gpt-5.2-codex": {
        "backend_alias": "qwen36-nothink-general",
        "description": "Mapped to the local Qwen non-thinking preset for general work.",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": MINI_EFFORTS,
        "priority": 1,
    },
}

LEGACY_MODELS: dict[str, dict[str, Any]] = {
    model_id: {
        "backend_alias": model_id,
        "description": preset["description"],
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "medium", "description": "medium"}
        ],
        "priority": 0,
        "visibility": "hide",
    }
    for model_id, preset in INTERNAL_PRESETS.items()
}

MODEL_REGISTRY: dict[str, dict[str, Any]] = {
    **DISPLAY_MODELS,
    **LEGACY_MODELS,
}

SERVER_VERSION = "local-qwen-alias-proxy/0.1"


class ThreadingHTTPServer(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True


class ProxyHandler(http.server.BaseHTTPRequestHandler):
    upstream_base: str

    def do_GET(self) -> None:  # noqa: N802
        path = self._request_path()

        if path == "/healthz":
            self._send_json(200, {"status": "ok"})
            return

        if path == "/v1/models":
            self._send_json(200, self._models_response())
            return

        self._send_json(404, {"error": f"unsupported path: {self.path}"})

    def do_POST(self) -> None:  # noqa: N802
        if self._request_path() != "/v1/responses":
            self._send_json(404, {"error": f"unsupported path: {self.path}"})
            return

        try:
            content_length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self._send_json(400, {"error": "invalid Content-Length"})
            return

        try:
            raw_body = self.rfile.read(content_length)
            payload = json.loads(raw_body.decode("utf-8"))
        except json.JSONDecodeError as exc:
            self._send_json(400, {"error": f"invalid JSON body: {exc}"})
            return

        if not isinstance(payload, dict):
            self._send_json(400, {"error": "request body must be a JSON object"})
            return

        model = payload.get("model")
        if not isinstance(model, str) or not model:
            self._send_json(400, {"error": "request body must include a string model"})
            return

        model_config = MODEL_REGISTRY.get(model)
        if model_config is None:
            self._send_json(
                400,
                {
                    "error": (
                        f"unsupported alias: {model}. "
                        f"supported aliases: {', '.join(MODEL_REGISTRY)}"
                    )
                },
            )
            return

        preset = INTERNAL_PRESETS[model_config["backend_alias"]]
        upstream_payload = copy.deepcopy(payload)
        upstream_payload["model"] = model_config["backend_alias"]
        upstream_payload["temperature"] = preset["temperature"]
        upstream_payload["top_p"] = preset["top_p"]
        upstream_payload["top_k"] = preset["top_k"]
        upstream_payload["min_p"] = preset["min_p"]
        upstream_payload["presence_penalty"] = preset["presence_penalty"]
        upstream_payload["reasoning_format"] = preset["reasoning_format"]
        self._clamp_reasoning_effort(
            upstream_payload,
            model_config["supported_reasoning_levels"],
            model_config["default_reasoning_level"],
        )
        self._normalize_instructions(upstream_payload)
        self._normalize_input_messages(upstream_payload)

        chat_template_kwargs = upstream_payload.get("chat_template_kwargs")
        if not isinstance(chat_template_kwargs, dict):
            chat_template_kwargs = {}
        chat_template_kwargs["enable_thinking"] = preset["enable_thinking"]
        upstream_payload["chat_template_kwargs"] = chat_template_kwargs

        try:
            self._forward_json(upstream_payload)
        except Exception as exc:  # noqa: BLE001
            self._send_json(502, {"error": f"upstream request failed: {exc}"})

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A003
        sys.stderr.write(f"{self.address_string()} - {format % args}\n")

    def _forward_json(self, payload: dict[str, Any]) -> None:
        upstream = urllib.parse.urlparse(f"{self.upstream_base}/v1/responses")
        connection_cls = (
            http.client.HTTPSConnection
            if upstream.scheme == "https"
            else http.client.HTTPConnection
        )
        conn = connection_cls(upstream.hostname, upstream.port, timeout=600)

        headers = {
            "Content-Type": "application/json",
            "Accept": self.headers.get("Accept", "*/*"),
        }
        if "OpenAI-Beta" in self.headers:
            headers["OpenAI-Beta"] = self.headers["OpenAI-Beta"]

        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        conn.request("POST", upstream.path, body=body, headers=headers)
        upstream_response = conn.getresponse()

        if upstream_response.status >= 400:
            input_preview = payload.get("input")
            if isinstance(input_preview, list):
                input_preview = input_preview[:4]
            error_body = upstream_response.read()
            sys.stderr.write(
                json.dumps(
                    {
                        "model": payload.get("model"),
                        "status": upstream_response.status,
                        "instructions": payload.get("instructions"),
                        "input_preview": input_preview,
                        "body": error_body.decode("utf-8", errors="replace"),
                    },
                    ensure_ascii=False,
                )
                + "\n"
            )
            self.send_response(upstream_response.status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(error_body)))
            self.end_headers()
            self.wfile.write(error_body)
            conn.close()
            return

        self.send_response(upstream_response.status)
        for key, value in upstream_response.getheaders():
            lowered = key.lower()
            if lowered in {"connection", "keep-alive", "transfer-encoding"}:
                continue
            self.send_header(key, value)
        self.end_headers()

        while True:
            chunk = upstream_response.read(64 * 1024)
            if not chunk:
                break
            self.wfile.write(chunk)
            self.wfile.flush()

        conn.close()

    def _send_json(self, status: int, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _request_path(self) -> str:
        return urllib.parse.urlparse(self.path).path

    @staticmethod
    def _models_response() -> dict[str, Any]:
        models = []
        for model_id, preset in DISPLAY_MODELS.items():
            models.append(
                {
                    "slug": model_id,
                    "display_name": model_id,
                    "description": preset["description"],
                    "default_reasoning_level": preset["default_reasoning_level"],
                    "supported_reasoning_levels": preset["supported_reasoning_levels"],
                    "shell_type": "shell_command",
                    "visibility": "list",
                    "minimal_client_version": [0, 0, 0],
                    "supported_in_api": True,
                    "priority": preset["priority"],
                    "upgrade": None,
                    "base_instructions": "",
                    "supports_reasoning_summaries": True,
                    "default_reasoning_summary": "none",
                    "support_verbosity": False,
                    "default_verbosity": None,
                    "apply_patch_tool_type": None,
                    "truncation_policy": {"mode": "tokens", "limit": 10000},
                    "supports_parallel_tool_calls": False,
                    "supports_image_detail_original": False,
                    "context_window": 131072,
                    "experimental_supported_tools": [],
                }
            )
        for model_id, preset in LEGACY_MODELS.items():
            models.append(
                {
                    "slug": model_id,
                    "display_name": model_id,
                    "description": preset["description"],
                    "default_reasoning_level": preset["default_reasoning_level"],
                    "supported_reasoning_levels": preset["supported_reasoning_levels"],
                    "shell_type": "shell_command",
                    "visibility": preset.get("visibility", "list"),
                    "minimal_client_version": [0, 0, 0],
                    "supported_in_api": True,
                    "priority": preset["priority"],
                    "upgrade": None,
                    "base_instructions": "",
                    "supports_reasoning_summaries": True,
                    "default_reasoning_summary": "none",
                    "support_verbosity": False,
                    "default_verbosity": None,
                    "apply_patch_tool_type": None,
                    "truncation_policy": {"mode": "tokens", "limit": 10000},
                    "supports_parallel_tool_calls": False,
                    "supports_image_detail_original": False,
                    "context_window": 131072,
                    "experimental_supported_tools": [],
                }
            )
        return {"models": models}

    @staticmethod
    def _clamp_reasoning_effort(
        payload: dict[str, Any],
        supported_levels: list[dict[str, Any]],
        default_level: str,
    ) -> None:
        reasoning = payload.get("reasoning")
        if not isinstance(reasoning, dict):
            return

        effort = reasoning.get("effort")
        if not isinstance(effort, str):
            return

        supported = [
            level.get("effort")
            for level in supported_levels
            if isinstance(level, dict) and isinstance(level.get("effort"), str)
        ]
        if not supported:
            return
        if effort in supported:
            return

        ordering = {
            "none": 0,
            "minimal": 1,
            "low": 2,
            "medium": 3,
            "high": 4,
            "xhigh": 5,
        }
        target_rank = ordering.get(effort, ordering.get(default_level, 3))
        replacement = min(
            supported,
            key=lambda candidate: abs(
                ordering.get(candidate, ordering.get(default_level, 3)) - target_rank
            ),
        )
        reasoning["effort"] = replacement

    @staticmethod
    def _normalize_instructions(payload: dict[str, Any]) -> None:
        instructions = payload.get("instructions")
        if not isinstance(instructions, str):
            payload.pop("instructions", None)
            return

        if not instructions.strip():
            payload.pop("instructions", None)
            return

        input_value = payload.get("input")
        system_message = {
            "type": "message",
            "role": "system",
            "content": [{"type": "input_text", "text": instructions}],
        }

        if isinstance(input_value, list):
            system_index = None
            for index, item in enumerate(input_value):
                if (
                    isinstance(item, dict)
                    and item.get("type") == "message"
                    and item.get("role") == "system"
                ):
                    system_index = index
                    break

            if system_index is not None:
                system_item = input_value.pop(system_index)
                content = system_item.get("content")
                if isinstance(content, list):
                    content.insert(0, {"type": "input_text", "text": instructions})
                else:
                    system_item["content"] = [{"type": "input_text", "text": instructions}]
                input_value.insert(0, system_item)
            else:
                input_value.insert(0, system_message)
        elif isinstance(input_value, str):
            payload["input"] = [
                system_message,
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": input_value}],
                },
            ]
        else:
            payload["input"] = [system_message]

        payload.pop("instructions", None)

    @staticmethod
    def _normalize_input_messages(payload: dict[str, Any]) -> None:
        input_value = payload.get("input")
        if not isinstance(input_value, list):
            return

        system_items = []
        other_items = []
        for item in input_value:
            if (
                isinstance(item, dict)
                and item.get("type") == "message"
                and item.get("role") == "developer"
            ):
                item = copy.deepcopy(item)
                item["role"] = "system"
            if (
                isinstance(item, dict)
                and item.get("type") == "message"
                and item.get("role") == "system"
            ):
                system_items.append(item)
            else:
                other_items.append(item)

        if system_items:
            merged_content: list[dict[str, Any]] = []
            for system_item in system_items:
                content = system_item.get("content")
                if isinstance(content, list):
                    merged_content.extend(copy.deepcopy(content))
            if merged_content:
                payload["input"] = [
                    {
                        "type": "message",
                        "role": "system",
                        "content": merged_content,
                    }
                ] + other_items
            else:
                payload["input"] = system_items[:1] + other_items


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--listen-host", default="127.0.0.1")
    parser.add_argument("--listen-port", type=int, default=8080)
    parser.add_argument("--upstream-base", default="http://127.0.0.1:12434")
    args = parser.parse_args()

    ProxyHandler.upstream_base = args.upstream_base.rstrip("/")

    server = ThreadingHTTPServer((args.listen_host, args.listen_port), ProxyHandler)
    print(
        json.dumps(
            {
                "listen": f"http://{args.listen_host}:{args.listen_port}",
                "upstream": ProxyHandler.upstream_base,
                "aliases": list(MODEL_REGISTRY),
            },
            ensure_ascii=False,
        ),
        flush=True,
    )
    server.serve_forever()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
