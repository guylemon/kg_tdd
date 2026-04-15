#!/usr/bin/env bash
set -euo pipefail

expected_path="${KG_ENTITY_EXPECTED_PATH:-tests/fixtures/gold/seed/expected_extraction.json}"
input_path="${1:-${KG_ENTITY_INPUT_PATH:-tests/fixtures/gold/seed/input.txt}}"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ ! -f "$expected_path" ]]; then
  echo "expected extraction file not found: $expected_path" >&2
  exit 1
fi

if [[ ! -f "$input_path" ]]; then
  echo "input file not found: $input_path" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

actual_response_path="$tmp_dir/response.json"
actual_entities_path="$tmp_dir/actual-entities.jsonl"
expected_entities_path="$tmp_dir/expected-entities.jsonl"
missing_path="$tmp_dir/missing.jsonl"
unexpected_path="$tmp_dir/unexpected.jsonl"

bash ./curl_test.sh "$input_path" >"$actual_response_path"

jq -r '
  .choices[0].message.content
  | if type == "string" then fromjson else . end
  | .entities[]
  | {name, entity_type, description}
  | @json
' "$actual_response_path" | sort >"$actual_entities_path"

jq -r '
  .entities[]
  | {name, entity_type, description}
  | @json
' "$expected_path" | sort >"$expected_entities_path"

echo "REASONING:"
jq -r '.choices[0].message.reasoning' "$actual_response_path"

comm -23 "$expected_entities_path" "$actual_entities_path" >"$missing_path"
comm -13 "$expected_entities_path" "$actual_entities_path" >"$unexpected_path"

if [[ ! -s "$missing_path" && ! -s "$unexpected_path" ]]; then
  echo "Entity extraction matched expected seed entities."
  exit 0
fi

echo ""
echo "RESULTS:"
echo "Entity extraction did not match expected seed entities."

if [[ -s "$missing_path" ]]; then
  echo
  echo "Missing entities:"
  cat "$missing_path"
fi

if [[ -s "$unexpected_path" ]]; then
  echo
  echo "Unexpected entities:"
  cat "$unexpected_path"
fi

exit 1
