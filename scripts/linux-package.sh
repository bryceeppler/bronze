#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

DOCKER_BUILDKIT=1 docker build --file scripts/linux-package.Dockerfile --build-arg SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)" --output type=local,dest=target/package .
ls target/package/bronze-*-linux-x86_64.tar.gz
