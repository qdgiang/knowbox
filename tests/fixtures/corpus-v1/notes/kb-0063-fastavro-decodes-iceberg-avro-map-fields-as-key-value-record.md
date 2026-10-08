---
id: kb-0063
title: "fastavro decodes Iceberg Avro map fields as {key,value} record arrays, not dicts"
type: gotcha
domain: projects
tags: [fastavro,iceberg,avro,manifest,python]
aliases: [fastavro map decode, iceberg manifest bounds, key-value record arrays, lower_bounds upper_bounds]
triggers:
  - "parsing Iceberg manifest or manifest-list Avro files with fastavro"
  - "lower_bounds/upper_bounds elements come back as dicts with key/value fields"
  - "TypeError unhashable type dict on Avro-decoded bounds"
status: active
confidence: medium
created: 2026-08-19
updated: 2026-08-19
supersedes: []
related: [kb-0043, kb-0007]
source: "raw/session-distill-2026-08-19.md"
summary: "fastavro reads Iceberg Avro map fields (lower_bounds/upper_bounds) as lists of {key,value} record dicts, not Python dicts — unwrap via comprehension before any dict operation."
---
When reading Iceberg's Avro metadata files (manifest list, manifests) with fastavro, map-typed fields such as `lower_bounds` and `upper_bounds` decode as lists of `{"key": ..., "value": ...}` records, not Python dicts. Iceberg's Avro schema encodes these fields as arrays of key-value records rather than native Avro maps, and fastavro mirrors the schema literally.

Consequence: code that assumes a dict breaks — e.g. `set(list(lows))` raises `TypeError: unhashable type: 'dict'` because each element is itself a dict, not a hashable value.

Unwrap recipe:

```python
bounds = {d["key"]: d["value"] for d in raw_bounds}
```

Discovered in experiment 01 of the Iceberg deep-dive series while inspecting manifest files downloaded from MinIO in the e2e lakehouse stack.

## Related
- [[kb-0043]] — Iceberg deep-dive curriculum; this gotcha surfaced in experiment 01
- [[kb-0007]] — e2e-data-ai-project map; the lakehouse stack whose manifests were inspected
