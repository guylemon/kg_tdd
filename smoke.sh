export RUST_LOG=debug
export KG_DEBUG_RAW_PROVIDER=1

cargo run -- \
  --input tests/fixtures/gold/seed/input.txt \
  --output-dir out \
  --provider-mode openai-compatible \
  --provider-base-url http://studio:11434 \
  --provider-model 'gemma4:e4b'

