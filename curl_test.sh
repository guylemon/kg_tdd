#!/usr/bin/env bash
set -euo pipefail

chat_url="${KG_PROVIDER_CHAT_URL:-http://studio:11434/v1/chat/completions}"
model="${KG_PROVIDER_MODEL:-gemma4:e4b}"
system_prompt_path="${KG_ENTITY_SYSTEM_PROMPT_PATH:-assets/prompts/entity.system.txt}"
input_path="${1:-${KG_ENTITY_INPUT_PATH:-tests/fixtures/gold/seed/input.txt}}"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ ! -f "$system_prompt_path" ]]; then
  echo "system prompt file not found: $system_prompt_path" >&2
  exit 1
fi

if [[ ! -f "$input_path" ]]; then
  echo "input file not found: $input_path" >&2
  exit 1
fi

system_prompt="$(cat "$system_prompt_path")"
input_text="$(cat "$input_path")"

jq -n \
  --arg model "$model" \
  --arg system_prompt "$system_prompt" \
  --arg input_text "$input_text" \
  '{
    model: $model,
    messages: [
      {
        role: "system",
        content: $system_prompt
      },
      {
        role: "user",
        content: $input_text
      }
    ],
    temperature: 0.0,
    response_format: {
      type: "json_schema",
      json_schema: {
        name: "AiExtractionResponse",
        strict: true,
        schema: {
          type: "object",
          additionalProperties: false,
          properties: {
            entities: {
              type: "array",
              items: {
                type: "object",
                additionalProperties: false,
                properties: {
                  name: {
                    type: "string"
                  },
                  entity_type: {
                    type: "string",
                    enum: [
                      "Concept",
                      "Event",
                      "Lifeform",
                      "Location",
                      "Organization",
                      "Person",
                      "Product",
                      "Technology"
                    ]
                  },
                  description: {
                    type: "string"
                  }
                },
                required: ["name", "entity_type", "description"]
              }
            }
          },
          required: ["entities"]
        }
      }
    },
    max_tokens: 512
  }' \
  | curl -sS "$chat_url" \
      -H 'Content-Type: application/json' \
      --data-binary @-
