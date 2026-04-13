#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/out"
exec python3 -m http.server 3000 --bind 0.0.0.0
