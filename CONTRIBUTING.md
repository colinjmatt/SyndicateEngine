# Contributing to SyndicateEngine

SyndicateEngine is a clean-room engine project. Please do not contribute copyrighted original game data, decompiled source, or copied proprietary implementation details.

## Local validation

Use the root `Makefile` for repeatable checks:

```bash
make validate
```

This runs formatting, tests, build, and regenerates the local asset report from `original_assets/` when present.

Individual commands:

```bash
make fmt
make test
make build
make report
make run
```

## Asset handling

- Keep legally owned original files in `original_assets/`.
- `original_assets/` is ignored by git and must stay out of commits.
- Generated reports must not include copyrighted binary payload bytes.

## Reverse engineering policy

- Prefer clean-room observations, tests, and independently written decoders.
- Document findings in `docs/reverse-engineering.md`.
- Add tests for every decoder assumption.