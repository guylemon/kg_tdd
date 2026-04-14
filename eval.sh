#!/usr/bin/env bash

set -euo pipefail

export KG_EVAL_PROVIDER_BASE_URL=http://studio:11434
export KG_EVAL_PROVIDER_MODEL=gemma4:e4b

cargo test --test eval_gold -- --ignored

