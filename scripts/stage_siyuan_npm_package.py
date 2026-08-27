#!/usr/bin/env python3
"""Stage and pack the Siyuan Codex npm packages for Linux and macOS.

Produces one self-contained tarball per target triple (a "platform package"
carrying only that platform's native binary, tagged with `os`/`cpu` so npm
skips non-matching platforms) plus a small "root wrapper" package that exposes
the `codex`/`siyuan` bins and pulls in the matching platform package through
optionalDependencies. Splitting the binaries across platform packages keeps each
tarball well under npm's large-package limits.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_TARGET_TRIPLE = "x86_64-unknown-linux-musl"
ROOT_PACKAGE_NAME = "siyuan-codex"

# Target triple -> platform package metadata (npm name, os, cpu).
PLATFORM_PACKAGES = {
    "x86_64-unknown-linux-musl": {
        "name": "siyuan-codex-linux-x64",
        "os": ["linux"],
        "cpu": ["x64"],
    },
    "aarch64-unknown-linux-musl": {
        "name": "siyuan-codex-linux-arm64",
        "os": ["linux"],
        "cpu": ["arm64"],
    },
    "x86_64-apple-darwin": {
        "name": "siyuan-codex-darwin-x64",
        "os": ["darwin"],
        "cpu": ["x64"],
    },
    "aarch64-apple-darwin": {
        "name": "siyuan-codex-darwin-arm64",
        "os": ["darwin"],
        "cpu": ["arm64"],
    },
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--version",
        required=True,
        help="Siyuan package version to stage.",
    )
    parser.add_argument(
        "--vendor-root",
        type=Path,
        required=True,
        help="Directory containing canonical Codex packages named by target triple.",
    )
    parser.add_argument(
        "--target",
        action="append",
        choices=sorted(PLATFORM_PACKAGES),
        default=[],
        help=(
            "Target triple to include. May be passed more than once. "
            f"Default: {DEFAULT_TARGET_TRIPLE}."
        ),
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=REPO_ROOT / "dist" / "siyuan-npm",
        help="Directory where the npm tarballs should be written.",
    )
    return parser.parse_args()


def copy_if_exists(src: Path, dest: Path) -> None:
    if src.exists():
        shutil.copy2(src, dest)


def make_executable(path: Path) -> None:
    path.chmod(path.stat().st_mode | 0o755)


def write_launcher(path: Path) -> None:
    path.write_text(
        """#!/usr/bin/env node
import { spawn } from "node:child_process";
import { existsSync, realpathSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const require = createRequire(import.meta.url);
const packageRoot = realpathSync(path.join(__dirname, ".."));

const PLATFORM_PACKAGE_BY_TARGET = {
  "x86_64-unknown-linux-musl": "siyuan-codex-linux-x64",
  "aarch64-unknown-linux-musl": "siyuan-codex-linux-arm64",
  "x86_64-apple-darwin": "siyuan-codex-darwin-x64",
  "aarch64-apple-darwin": "siyuan-codex-darwin-arm64",
};

const TARGET_BY_PLATFORM_AND_ARCH = {
  linux: {
    x64: "x86_64-unknown-linux-musl",
    arm64: "aarch64-unknown-linux-musl",
  },
  darwin: {
    x64: "x86_64-apple-darwin",
    arm64: "aarch64-apple-darwin",
  },
};

function targetTripleFor(platform, arch) {
  const platformTargets = TARGET_BY_PLATFORM_AND_ARCH[platform];
  return platformTargets ? platformTargets[arch] : undefined;
}

function findCodexInVendor(vendorRoot, targetTriple) {
  const candidates = [
    path.join(vendorRoot, "vendor", targetTriple, "bin", "codex"),
    path.join(vendorRoot, "vendor", targetTriple, "codex", "codex"),
  ];
  return candidates.find((candidate) => existsSync(candidate));
}

function findBundledCodex() {
  const targetTriple = targetTripleFor(process.platform, process.arch);
  if (!targetTriple) {
    throw new Error(
      `Unsupported platform for siyuan-codex: ${process.platform}/${process.arch}`,
    );
  }

  // Prefer the matching platform package pulled in by the siyuan-codex root
  // wrapper's optionalDependencies.
  const platformPackage = PLATFORM_PACKAGE_BY_TARGET[targetTriple];
  if (platformPackage) {
    try {
      const packageJsonPath = require.resolve(`${platformPackage}/package.json`);
      const fromPlatform = findCodexInVendor(
        path.join(path.dirname(packageJsonPath), ".."),
        targetTriple,
      );
      if (fromPlatform) {
        return fromPlatform;
      }
    } catch {
      // Platform package is not installed; fall back to a bundled vendor.
    }
  }

  const local = findCodexInVendor(packageRoot, targetTriple);
  if (local) {
    return local;
  }

  throw new Error(
    `Missing bundled Siyuan Codex binary for ${targetTriple}. Reinstall with: npm install -g siyuan-codex@latest`,
  );
}

const child = spawn(findBundledCodex(), process.argv.slice(2), {
  stdio: "inherit",
  env: {
    ...process.env,
    CODEX_MANAGED_BY_NPM: "1",
    CODEX_MANAGED_PACKAGE_ROOT: realpathSync(packageRoot),
  },
});

const forwardSignal = (signal) => {
  if (!child.killed) {
    child.kill(signal);
  }
};

process.on("SIGINT", forwardSignal);
process.on("SIGTERM", forwardSignal);

child.on("error", (error) => {
  console.error(error.message);
  process.exit(1);
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
""",
        encoding="utf-8",
    )
    make_executable(path)


def copy_common_sources(staging_dir: Path) -> None:
    copy_if_exists(REPO_ROOT / "README.md", staging_dir / "README.md")
    copy_if_exists(REPO_ROOT / "LICENSE", staging_dir / "LICENSE")


def stage_platform_package(
    staging_dir: Path, vendor_root: Path, version: str, target: str
) -> None:
    pkg = PLATFORM_PACKAGES[target]
    target_vendor = vendor_root.resolve() / target
    if not target_vendor.exists():
        raise RuntimeError(f"Missing vendor target: {target_vendor}")
    if not (target_vendor / "codex" / "codex").exists():
        raise RuntimeError(
            f"Missing Codex binary in vendor target: {target_vendor / 'codex' / 'codex'}"
        )

    staging_dir.mkdir(parents=True, exist_ok=True)
    vendor_dest = staging_dir / "vendor"
    shutil.copytree(target_vendor, vendor_dest / target)
    make_executable(vendor_dest / target / "codex" / "codex")
    sandbox_path = vendor_dest / target / "path" / "codex-linux-sandbox"
    if sandbox_path.exists():
        make_executable(sandbox_path)

    package_json = {
        "name": pkg["name"],
        "version": version,
        "description": f"Siyuan-branded Codex CLI ({target}).",
        "license": "Apache-2.0",
        "os": pkg["os"],
        "cpu": pkg["cpu"],
        "engines": {
            "node": ">=16",
        },
        "files": [
            "vendor",
            "README.md",
            "LICENSE",
        ],
        "repository": {
            "type": "git",
            "url": "git+https://github.com/redyuan43/codex.git",
        },
    }
    with open(staging_dir / "package.json", "w", encoding="utf-8") as out:
        json.dump(package_json, out, indent=2)
        out.write("\n")

    copy_common_sources(staging_dir)


def stage_root_package(staging_dir: Path, version: str, targets: list[str]) -> None:
    staging_dir.mkdir(parents=True, exist_ok=True)
    bin_dir = staging_dir / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    write_launcher(bin_dir / "codex.js")

    package_json = {
        "name": ROOT_PACKAGE_NAME,
        "version": version,
        "description": "Siyuan-branded Codex CLI.",
        "license": "Apache-2.0",
        "type": "module",
        "bin": {
            "codex": "bin/codex.js",
            "siyuan": "bin/codex.js",
        },
        "engines": {
            "node": ">=16",
        },
        "files": [
            "bin/codex.js",
            "README.md",
            "LICENSE",
        ],
        "repository": {
            "type": "git",
            "url": "git+https://github.com/redyuan43/codex.git",
        },
        "optionalDependencies": {
            PLATFORM_PACKAGES[target]["name"]: version for target in targets
        },
    }
    with open(staging_dir / "package.json", "w", encoding="utf-8") as out:
        json.dump(package_json, out, indent=2)
        out.write("\n")

    copy_common_sources(staging_dir)


def run_npm_pack(staging_dir: Path, output_dir: Path) -> Path:
    output_dir = output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="siyuan-codex-npm-pack-") as pack_dir_str:
        pack_dir = Path(pack_dir_str)
        npm_cache_dir = pack_dir / "npm-cache"
        npm_logs_dir = pack_dir / "npm-logs"
        npm_cache_dir.mkdir()
        npm_logs_dir.mkdir()
        env = os.environ.copy()
        env["NPM_CONFIG_CACHE"] = str(npm_cache_dir)
        env["NPM_CONFIG_LOGS_DIR"] = str(npm_logs_dir)
        stdout = subprocess.check_output(
            ["npm", "pack", "--json", "--pack-destination", str(pack_dir)],
            cwd=staging_dir,
            env=env,
            text=True,
        )
        pack_output = json.loads(stdout)
        if not pack_output:
            raise RuntimeError("npm pack did not produce an output tarball.")
        tarball_name = pack_output[0].get("filename") or pack_output[0].get("name")
        if not tarball_name:
            raise RuntimeError("Unable to determine npm pack output filename.")
        tarball_path = pack_dir / tarball_name
        if not tarball_path.exists():
            raise RuntimeError(f"Expected npm pack output not found: {tarball_path}")
        output_path = output_dir / tarball_name
        shutil.move(str(tarball_path), output_path)

    return output_path


def main() -> int:
    args = parse_args()
    targets = args.target or [DEFAULT_TARGET_TRIPLE]
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    produced: list[Path] = []
    with tempfile.TemporaryDirectory(prefix="siyuan-codex-stage-") as tmp_dir_str:
        tmp_root = Path(tmp_dir_str)
        for target in targets:
            staging_dir = tmp_root / PLATFORM_PACKAGES[target]["name"]
            stage_platform_package(staging_dir, args.vendor_root, args.version, target)
            produced.append(run_npm_pack(staging_dir, output_dir))

        root_staging_dir = tmp_root / "root"
        stage_root_package(root_staging_dir, args.version, targets)
        produced.append(run_npm_pack(root_staging_dir, output_dir))

    for path in produced:
        print(f"npm pack output written to {path}")
    print(
        f"Staged {ROOT_PACKAGE_NAME}@{args.version} for targets: {', '.join(targets)}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
