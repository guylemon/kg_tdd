# Gold Fixtures

Each gold fixture lives in its own directory and must contain:

- `input.txt`: the curated source document for the scenario
- `expected.json`: the human-reviewed expected graph semantics

`expected.json` is the canonical source of truth for the scenario. It is intentionally smaller
than the runtime `graph.json` artifact and focuses on domain semantics:

- canonical node identities, names, entity types, and aliases
- canonical edge identities, endpoints, relationship types, descriptions, and weights
- evidence summaries with `fact`, `citation_text`, and `status`

These fixtures are curated for regression and evaluation work. Runtime viewer projection details
and mention-level payloads are intentionally excluded from the gold expectation format.
