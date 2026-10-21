#!/usr/bin/env python3
"""Build the nonshipping macOS fixture with one explicit Tauri config overlay.

The production frontend must already be built. This helper builds only; it
never packages, registers or launches an application and never reads a profile.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time

APP_ID = 'studio.orbitpal.native-world-fixture'
NULL_BUILD_KEYS = ('devUrl', 'beforeDevCommand', 'beforeBuildCommand', 'beforeBundleCommand')


def require(condition):
    if not condition:
        raise ValueError('native_world_fixture_build_refused')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence-dir', required=True, type=Path)
    args = parser.parse_args()
    require(sys.platform == 'darwin')
    root = Path(__file__).resolve().parents[2]
    evidence = args.evidence_dir
    require(evidence.is_absolute() and evidence.resolve(strict=True) == evidence)
    meta = evidence.stat()
    require(stat.S_ISDIR(meta.st_mode) and meta.st_uid == os.geteuid() and meta.st_mode & 0o022 == 0)
    config_path = root / 'src-tauri/tauri.native-world-fixture.conf.json'
    raw = config_path.read_bytes()
    require(0 < len(raw) <= 32_768)
    config = json.loads(raw)
    build = config.get('build', {})
    windows = config.get('app', {}).get('windows', [])
    require(config.get('identifier') == APP_ID and config.get('bundle', {}).get('active') is False)
    require(build.get('frontendDist') == '../dist'
            and all(key in build and build[key] is None for key in NULL_BUILD_KEYS))
    require('plugins' in config and config['plugins'] is None)
    require(len(windows) == 1 and windows[0].get('label') == 'main'
            and windows[0].get('incognito') is True and windows[0].get('visible') is False)
    index = root / 'dist/index.html'
    require(index.is_file() and index.stat().st_size > 0)
    # Both tauri-build and generate_context! consume this same inherited JSON
    # merge patch. Null values remove shipping dev hooks and plugin config.
    overlay = json.dumps(config, ensure_ascii=True, sort_keys=True, separators=(',', ':'))
    environment = os.environ.copy()
    environment['TAURI_CONFIG'] = overlay
    environment['ORBITPAL_NATIVE_WORLD_FIXTURE_EVIDENCE'] = str(evidence)
    command = ['cargo', 'build', '--manifest-path', str(root / 'src-tauri/Cargo.toml'),
               '--locked', '--jobs', '1', '--features', 'native-world-fixture,tauri/custom-protocol',
               '--example', 'native_world_fixture']
    # Print only this fixed command and public config fingerprints, never the
    # environment or its credential/toolchain values. Cargo output is inherited.
    print(json.dumps({'stage': 'fixture_build_started', 'command': command,
                      'config_sha256': hashlib.sha256(raw).hexdigest(),
                      'tauri_config_sha256': hashlib.sha256(overlay.encode()).hexdigest(),
                      'identifier': APP_ID, 'evidence_dir': str(evidence)}), flush=True)
    started = time.monotonic()
    result = subprocess.run(command, cwd=root, env=environment, check=False)
    print(json.dumps({'stage': 'fixture_build_finished', 'returncode': result.returncode,
                      'duration_ms': int((time.monotonic() - started) * 1000)}), flush=True)
    return result.returncode if result.returncode >= 0 else 128 - result.returncode


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, TypeError, KeyError):
        print('native_world_fixture_build_refused', file=sys.stderr)
        raise SystemExit(2)
