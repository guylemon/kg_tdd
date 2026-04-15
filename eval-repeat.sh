#!/usr/bin/env bash

set -euo pipefail

export KG_EVAL_PROVIDER_BASE_URL="${KG_EVAL_PROVIDER_BASE_URL:-http://studio:11434}"
export KG_EVAL_PROVIDER_MODEL="${KG_EVAL_PROVIDER_MODEL:-gemma4:e4b}"
export RUST_LOG=debug
export KG_DEBUG_RAW_PROVIDER=1

cargo test --test eval_gold gold_fixtures_remain_stable_across_repeated_runs -- --ignored --nocapture
