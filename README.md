# `kg_tdd`

`kg_tdd` is a standalone CLI that ingests a single text document and produces a static knowledge-graph artifact bundle.

The generated bundle is meant to be inspected locally as static files. The CLI is the product surface; internal modules and implementation details are expected to evolve.

## What It Produces

Each successful run writes an output directory containing:

- `graph.json`
- `index.html`
- `cytoscape.min.js`

This output layout is the current artifact contract for the CLI.

## Quick Start

Generate the artifact bundle:

```bash
cargo run -- --input fixtures/seed.txt --output-dir out
```

Or use the convenience script:

```bash
./run.sh
```

Serve the generated bundle locally:

```bash
./serve.sh
```

By default, `serve.sh` serves `out/` on `0.0.0.0:3000`. On a trusted local network, other devices can reach it at:

```text
http://<your-lan-ip>:3000
```

## CLI Interface

Current flags:

- `--input <PATH>`: source document to ingest
- `--output-dir <PATH>`: directory where artifacts will be written
- `--tokenizer <NAME>`: tokenizer identifier used for chunking
- `--max-chunk-tokens <N>`: chunk size limit

Example:

```bash
cargo run -- \
  --input path/to/document.txt \
  --output-dir out \
  --tokenizer bert-base-cased \
  --max-chunk-tokens 128
```

## Artifact Contract

The generated output directory is intended to be portable and inspectable as static files.

### `graph.json`

- Machine-readable graph artifact
- Produced by the CLI on every successful run
- Consumed by the static viewer

### `index.html`

- Static viewer entrypoint
- Loads only local assets
- Fetches `./graph.json`
- Loads `./cytoscape.min.js`

### `cytoscape.min.js`

- Vendored viewer runtime copied into the output bundle
- Source asset lives at [assets/viewer/cytoscape.min.js](/home/eci/dev/kg_tdd/assets/viewer/cytoscape.min.js:1)

Source viewer assets live under [assets/viewer](/home/eci/dev/kg_tdd/assets/viewer:1). The runtime output is copied from those repo-owned assets into the requested output directory.

## Local Viewing

Open the bundle through a local HTTP server rather than directly from `file://`, since the viewer fetches `graph.json`.

The minimal supported workflow is:

1. Generate the bundle into `out/`.
2. Run `./serve.sh`.
3. Open `http://127.0.0.1:3000` on the same machine, or `http://<your-lan-ip>:3000` from another device on the same trusted network.

## Development

Run tests with:

```bash
cargo test
```

Current test coverage includes:

- CLI argument parsing
- file-based document input
- graph artifact bundle generation
- end-to-end application flow

## Project Status

This repository is being developed incrementally against [ROADMAP.md](/home/eci/dev/kg_tdd/ROADMAP.md:1).

The stable surface to rely on is:

- the CLI invocation shape
- the generated artifact directory contract
- the local static-viewing workflow

Provider integrations, graph semantics, evaluation workflows, and viewer quality are expected to expand in later milestones.
