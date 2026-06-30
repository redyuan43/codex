#!/usr/bin/env python3
"""Stage and pack a Linux self-contained siyuan Codex npm package."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


REPO_ROOT = Path(__file__).resolve().parent.parent
CODEX_CLI_ROOT = REPO_ROOT / "codex-cli"
DEFAULT_TARGET_TRIPLE = "x86_64-unknown-linux-musl"
CPU_BY_TARGET_TRIPLE = {
    "x86_64-unknown-linux-musl": "x64",
    "aarch64-unknown-linux-musl": "arm64",
}
PACKAGE_NAME = "@ivanfeng3333/siyuan-codex"
DEFAULT_VERSION = "0.142.4-siyuan.6"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--version",
        default=DEFAULT_VERSION,
        help=f"npm package version to stage. Default: {DEFAULT_VERSION}.",
    )
    parser.add_argument(
        "--vendor-root",
        type=Path,
        required=True,
        help=("Directory containing canonical Codex packages named by target triple."),
    )
    parser.add_argument(
        "--target",
        action="append",
        choices=sorted(CPU_BY_TARGET_TRIPLE),
        default=[],
        help=(
            "Target triple to include. May be passed more than once. "
            f"Default: {DEFAULT_TARGET_TRIPLE}."
        ),
    )
    parser.add_argument(
        "--staging-dir",
        type=Path,
        help="Directory to stage package contents. Must be empty when provided.",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=REPO_ROOT / "dist" / "siyuan-npm",
        help="Directory where the npm tarball should be written.",
    )
    return parser.parse_args()


def prepare_staging_dir(staging_dir: Path | None) -> tuple[Path, bool]:
    if staging_dir is None:
        return Path(tempfile.mkdtemp(prefix="siyuan-codex-npm-")), True

    staging_dir = staging_dir.resolve()
    staging_dir.mkdir(parents=True, exist_ok=True)
    if any(staging_dir.iterdir()):
        raise RuntimeError(f"Staging directory is not empty: {staging_dir}")
    return staging_dir, False


def write_package_json(staging_dir: Path, version: str, targets: list[str]) -> None:
    cpus = sorted({CPU_BY_TARGET_TRIPLE[target] for target in targets})
    package_json = {
        "name": PACKAGE_NAME,
        "version": version,
        "description": "Siyuan-branded Codex CLI for Linux.",
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
            "vendor",
            "README.md",
            "LICENSE",
        ],
        "repository": {
            "type": "git",
            "url": "git+https://github.com/redyuan43/codex.git",
        },
        "os": [
            "linux",
        ],
        "cpu": cpus,
    }
    with open(staging_dir / "package.json", "w", encoding="utf-8") as out:
        json.dump(package_json, out, indent=2)
        out.write("\n")


def copy_if_exists(src: Path, dest: Path) -> None:
    if src.exists():
        shutil.copy2(src, dest)


def make_executable(path: Path) -> None:
    path.chmod(path.stat().st_mode | 0o755)


def stage_sources(
    staging_dir: Path, vendor_root: Path, version: str, targets: list[str]
) -> None:
    bin_dir = staging_dir / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(CODEX_CLI_ROOT / "bin" / "codex.js", bin_dir / "codex.js")

    for target in targets:
        target_vendor = vendor_root.resolve() / target
        if not target_vendor.exists():
            raise RuntimeError(f"Missing Linux vendor target: {target_vendor}")
        if not (target_vendor / "codex" / "codex").exists():
            raise RuntimeError(
                "Missing Codex binary in vendor target: "
                f"{target_vendor / 'codex' / 'codex'}"
            )

        vendor_dest = staging_dir / "vendor" / target
        vendor_dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(target_vendor, vendor_dest)
        make_executable(vendor_dest / "codex" / "codex")
        sandbox_path = vendor_dest / "path" / "codex-linux-sandbox"
        if sandbox_path.exists():
            make_executable(sandbox_path)

    copy_if_exists(REPO_ROOT / "README.md", staging_dir / "README.md")
    copy_if_exists(REPO_ROOT / "LICENSE", staging_dir / "LICENSE")
    write_package_json(staging_dir, version, targets)


def run_npm_pack(staging_dir: Path, output_dir: Path, version: str) -> Path:
    output_dir = output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / f"siyuan-codex-npm-{version}.tgz"

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
        shutil.move(str(tarball_path), output_path)

    return output_path


def main() -> int:
    args = parse_args()
    targets = args.target or [DEFAULT_TARGET_TRIPLE]
    staging_dir, created_temp = prepare_staging_dir(args.staging_dir)
    try:
        stage_sources(staging_dir, args.vendor_root, args.version, targets)
        output_path = run_npm_pack(staging_dir, args.output_dir, args.version)
        print(f"Staged {PACKAGE_NAME}@{args.version}")
        if not created_temp:
            print(f"Staging directory: {staging_dir}")
        print(f"npm pack output written to {output_path}")
    finally:
        if created_temp:
            shutil.rmtree(staging_dir, ignore_errors=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
