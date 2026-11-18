# Native integration-test fixtures

`fixtures/corpus-v1/` and `goldens/{index_build.txt,lint_report.json,search_ranking.json}` are
copied byte-for-byte from the
[legacy Python repository at `1da7ffdd`](https://github.com/qdgiang/knowbox-python/tree/1da7ffdd70b10d53295199ac8777612c8d90d49b).

The index golden contains the generated `index.md` bytes for the frozen
corpus. The search golden contains ranked note IDs only; SQLite scores and
snippets are intentionally not frozen because they are SQLite-version
details. Do not regenerate either golden as part of a Rust implementation
change without comparing the resulting diff to the legacy oracle.
