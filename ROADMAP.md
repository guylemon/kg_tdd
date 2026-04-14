# Roadmap Recommendation for the CLI Knowledge-Graph Product

## Summary

This project should be treated as a standalone CLI that ingests a single document and produces a static graph artifact bundle:

- `graph.json`
- `index.html`
- local `cytoscape.min.js`

The current repo proves a narrow vertical slice, but the product is still incomplete in several important areas beyond provider integration and stronger graph semantics.

The roadmap should expand across these five tracks:

1. CLI productization and artifact generation
2. Extraction/provider integration
3. Graph semantics and consolidation quality
4. Reliability, evaluation, and observability
5. Input robustness and format support

The main recommendation is to sequence reliability and evaluation before broad feature expansion. Without that, provider and graph-semantics work will be difficult to compare, debug, and improve safely.

## Milestone 1: CLI Product Completion

Status: complete as of April 13, 2026.

Completed in this milestone:

- Replaced the prototype `stdin -> stdout` flow with a real single-document CLI.
- Added explicit CLI flags for:
  - input document path
  - output directory
  - tokenizer name
  - max chunk tokens
- Switched the application to file-based input and artifact-oriented output.
- Added `graph.json` generation into the requested output directory.
- Added the static artifact bundle alongside `graph.json`:
  - `index.html`
  - local `cytoscape.min.js`
- Kept the viewer build-free and static. The page loads local JS and local JSON only.
- Defined the generated output directory layout as a stable artifact contract.
- Added clearer user-facing failures and exit codes for:
  - CLI usage errors
  - invalid input path
  - input read failure / empty input
  - tokenizer load failure
  - extraction failure
  - output directory / write failure
- Replaced the placeholder `stdin-document` identity with a path-derived document ID.
- Updated the developer convenience script to invoke the CLI through the new file-based interface.
- Added end-to-end tests that validate the full artifact bundle, not just `graph.json`.

Why this matters:
The generated directory is now directly inspectable as a static graph viewer rather than only a JSON artifact, so the CLI product surface for Milestone 1 is in place.

## Milestone 2: Provider Integration Layer

Status: complete as of April 13, 2026.

Completed in this milestone:

- Added a real provider adapter behind the existing schema client boundary.
- Added provider configuration and secrets handling for the CLI.
- Defined stable structured extraction schemas for:
  - entity extraction
  - relationship extraction
  - evidence/citation extraction
- Preserved the fake client as a deterministic test fixture path.
- Added provider retries, timeout handling, and per-call error classification.
- Added provider debugging logs for live schema-mismatch diagnosis, with raw prompt/response logging gated behind an explicit opt-in environment flag.

Why this matters:
Provider work now includes deterministic fixture parity, config handling, and operational error behavior rather than only raw API calls, so the provider integration layer is in place for subsequent quality and semantics work.

## Milestone 3: Graph Semantics and Consolidation

- [x] Replace current minimal dedupe by raw entity name with stronger node identity rules.
- [x] Add normalization and alias handling for common variants.
  - Added a dedicated domain canonicalization layer with conservative normalization, `&`/`and` handling, and curated organization suffix variants.
- [x] Revisit relationship consolidation so edges are not keyed only by `(source, target)` when relationship type or meaning differs.
  - Relationship consolidation now keys edges by `(source, target, relationship_type)`, preserving distinct edge types between the same nodes while still merging same-type duplicates.
- [x] Strengthen evidence handling:
  - Citation provenance is now preserved at the source-chunk `TextUnit` level for accepted evidence.
  - Epistemic status policy is now explicit and centralized: grounded evidence defaults to `Probable`.
  - Duplicate evidence is now merged during relationship consolidation using normalized fact/citation text plus chunk provenance.
- [x] Make node and edge IDs domain-derived and stable under repeated runs.

Why this matters:
“Stronger semantics” should be split into explicit subproblems: identity, normalization, and evidence policy.

## Milestone 4: Reliability and Evaluation

- [x] Add gold-style fixture documents and expected graph outputs.
- [ ] Introduce evaluation harnesses for:
  - [ ] extraction correctness
    - Opt-in real-provider harness scaffolding now exists, but prompt behavior is still too unstable for the harness to pass reliably or define a durable regression boundary.
  - [ ] graph consolidation correctness
    - Domain-graph comparison scaffolding now exists, but it is not yet verified as a stable pass/fail contract under live-provider behavior.
  - [ ] end-to-end regression detection
    - Current evaluation stops at normalized domain-graph comparison. This is sufficient as a v1 working boundary, but it is too early to decide whether it is the final end-to-end regression boundary for the product.
- [x] Add traceable intermediate artifacts for debugging:
  - [x] chunk list
  - [x] raw provider responses
  - [x] extracted mentions before consolidation
  - The traced ingestion pipeline now carries chunk metadata, per-schema raw provider payloads, and pre-consolidation extracted mentions through a shared application result.
  - Successful CLI runs now emit these artifacts under `debug/` as `chunk-list.json`, `raw-provider-responses.json`, and `extracted-mentions.json` alongside the graph bundle.
  - The gold evaluation harness now reuses the traced pipeline and writes a temporary debug artifact bundle on mismatch, reporting the artifact path in the failure output.
- [x]  Add structured logging and run metadata.
- [x] Add deterministic test coverage for failure paths, not only happy paths.

Why this matters:
The project can run, but it cannot yet measure quality or safely evolve.

## Milestone 5: Prompt and Extraction Stabilization

- Stabilize extraction prompts so the gold evaluation harness can pass consistently enough to distinguish prompt churn from real regressions.
- Improve schema instructions and extraction prompts until entity, relationship, and evidence outputs are predictably shaped across the current gold fixture pack.
- Use the opt-in real-provider evaluation target to iterate on prompt quality, not yet as a strict release gate.
- Expand or refine gold fixture expectations only where needed to support prompt stabilization and clearer failure diagnosis.
- Once evaluation behavior is reasonably stable, explicitly decide whether normalized domain-graph comparison is the final regression boundary or whether the product needs a broader end-to-end boundary.
- Only after the step above, mark Milestone 4 item 2 and its subitems complete.

Why this matters:
Evaluation scaffolding exists, but prompt instability currently dominates the failures. The project needs a stabilization phase before gold-eval failures can be treated as reliable regressions.

## Milestone 6: Input and Ingestion Robustness

- Improve document identity beyond the current placeholder `stdin-document`.
- Add real file-based input handling and path-based document metadata.
- Support larger and noisier real-world text inputs.
- Add configurable chunking behavior beyond the current fixed-token default if needed.
- Decide how to treat unsupported or partial extraction results from individual chunks.

Why this matters:
The current IO layer is still prototype-level. Even with a provider integrated, ingest robustness is not product-ready.

## Milestone 7: Static Viewer Quality

- Add the actual static HTML viewer and package layout.
- Keep viewer logic intentionally simple:
  - load `graph.json`
  - initialize Cytoscape
  - basic readable styling
  - click-to-inspect node and edge metadata
- Ensure the generated directory can be served directly with `python -m http.server`.
- Treat visualization as a consumer of the graph artifact, not as a second graph model.

Why this matters:
The final product is not just “graph JSON exists”; it is “a person can open a static page and inspect the graph.”

## Important Interface and Type Changes

- Add a real CLI argument and config model for:
  - input document path
  - output directory
  - tokenizer name
  - max chunk tokens
  - provider mode or fixture mode
- Introduce artifact-oriented output interfaces instead of only writing one JSON string to stdout.
- Extend `AppError` into user-meaningful error categories rather than a mostly coarse internal enum.
- Add explicit intermediate extraction types that can be serialized for debugging and evaluation.
- Add a viewer artifact contract:
  - expected `graph.json` shape
  - expected static asset layout for `index.html` and `cytoscape.min.js`

## Test Plan

- CLI tests:
  - valid input file produces an output directory with expected files
  - invalid input path fails clearly
  - provider config errors fail clearly
- Artifact tests:
  - generated `graph.json` is valid
  - generated `index.html` references only local assets
- Extraction tests:
  - deterministic fixture path remains stable
  - provider adapters map responses into internal extraction types correctly
- Graph tests:
  - alias and normalization scenarios
  - relationship type collisions
  - duplicate evidence handling
- Evaluation tests:
  - fixture documents with expected graph assertions
  - regression cases across chunk boundaries
  - opt-in real-provider gold-eval target remains separate from default unit tests until prompt behavior is stable
- Viewer smoke test:
  - static bundle is structurally complete and can be served locally

## Assumptions and Defaults

- Final product is a standalone CLI, not a reusable public library.
- Primary workflow is single document to graph artifact.
- Main output is a static HTML viewer backed by local `graph.json` and local `cytoscape.min.js`.
- Reliability and evaluability are prioritized before broad format or output expansion.
- Provider integration and graph semantics remain core roadmap items, but they should not be pursued without the supporting workstreams above.
- Corpus ingestion, hosted service behavior, and advanced UX can remain later phases unless product goals change.
