#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
binaries_path="${1:-$(pwd)/target/release}"
test -x "$binaries_path/realflow"
test -x "$binaries_path/realflow-launcher"
mkdir -p release
build_stage="$(mktemp -d "$(pwd)/release/.realflow-stage.XXXXXX")"
trap 'rm -rf "$build_stage"' EXIT
package_path="$build_stage/RealFlow-Linux-x64"
mkdir -p "$package_path"
cp "$binaries_path/realflow" "$binaries_path/realflow-launcher" README.md LICENSE config.json "$package_path/"
cp -r examples "$package_path/"
(cd /tmp && "$package_path/realflow" --config "$package_path/config.json" doctor && "$package_path/realflow" --config "$package_path/config.json" demo --steps 240 --output "$package_path/demo-results.json" && "$package_path/realflow" --config "$package_path/config.json" atc-demo --steps 600 --output "$package_path/atc-demo-results.json")
tar -C "$build_stage" -czf "$build_stage/RealFlow-Linux-x64.tar.gz" RealFlow-Linux-x64
(cd "$build_stage" && sha256sum RealFlow-Linux-x64.tar.gz > SHA256SUMS.txt)
mv "$build_stage/RealFlow-Linux-x64.tar.gz" "$build_stage/SHA256SUMS.txt" release/
