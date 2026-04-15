# Diagnostics Scripts

These scripts are lightweight diagnostics for prompt and model smoke testing.

They are not the source of truth for evaluation. The Rust gold-eval flow is authoritative because it applies the repository's real normalization, consolidation, and graph-building logic.

## Scripts

`curl_test.sh`

- Sends one entity-extraction request directly to the configured OpenAI-compatible chat endpoint.
- Useful for inspecting raw provider responses while iterating on prompt text or swapping models.

Example:

```bash
bash scripts/diagnostics/curl_test.sh tests/fixtures/gold/seed/input.txt
```

`eval_entity_seed.sh`

- Compares extracted `{name, entity_type}` pairs against an expected extraction fixture.
- Useful as a fast smoke test, but it does not apply Rust-side canonicalization or consolidation semantics.

Example:

```bash
bash scripts/diagnostics/eval_entity_seed.sh
```

To run against another fixture:

```bash
KG_ENTITY_EXPECTED_PATH=tests/fixtures/gold/relationship-type-collision/expected_extraction.json \
  bash scripts/diagnostics/eval_entity_seed.sh tests/fixtures/gold/relationship-type-collision/input.txt
```

## Environment

- `KG_PROVIDER_CHAT_URL`: chat completions endpoint
- `KG_PROVIDER_MODEL`: model name
- `KG_ENTITY_SYSTEM_PROMPT_PATH`: override entity system prompt file
- `KG_ENTITY_INPUT_PATH`: default input path when no positional input is supplied
- `KG_ENTITY_EXPECTED_PATH`: expected extraction path for `eval_entity_seed.sh`

## Guidance

- Use these scripts for fast local troubleshooting.
- Use `cargo test --test eval_gold ... -- --ignored` for authoritative evaluation.
