---
id: kb-0074
title: "Iceberg S3 region config: s3.region is silently ignored, use client.region (REST catalogs survive because servers vend it)"
type: gotcha
domain: projects
tags: [iceberg, spark, minio, polaris, nessie, s3fileio]
aliases: [s3.region ignored, client.region, iceberg region property]
status: active
confidence: high
created: 2026-08-24
updated: 2026-08-24
supersedes: []
related: [kb-0043, kb-0007]
source: "/home/giang/projects/iceberg-learning/experiments/01b-catalog-snapshot-comparison/output.log"
summary: "Iceberg 1.11 ignores s3.region — S3FileIO reads region only from client.region. REST catalogs mask it (Polaris/Nessie vend client.region in loadTable); HadoopCatalog fails 'Unable to load region'."
---
`spark.sql.catalog.<c>.s3.region` does nothing on Iceberg 1.11 — S3FileIOProperties defines no region key; the S3 client region comes solely from `client.region` (org.apache.iceberg.aws.AwsClientProperties.CLIENT_REGION, verified by decompiling iceberg-spark-runtime-3.5_2.12-1.11.0.jar: DefaultAwsClientFactory.s3() applies awsClientProperties, constructor reads only "client.region").

Why the e2e lakehouse's REST catalogs work anyway: Polaris loadTable responses carry `"config": {"client.region": "us-east-1", "s3.endpoint": ..., "s3.path-style-access": ...}` (vended per table from storageConfigInfo); the Iceberg REST client merges that config into the FileIO properties. Nessie does the same (credentials/config vending). So the bogus `s3.region` key in `spark/jobs/lakehouse_catalog.py` is masked for nessie/polaris.

Failure mode: a non-REST catalog (HadoopCatalog via SparkCatalog `type=hadoop`) has no server to inject config — first commit dies with `SdkClientException: Unable to load region from any of the providers in the chain` (the EC2-metadata probe returns a proxy "Waiting..." HTML page inside the compose network, parsed as JSON → JsonParseException). Seen live 2026-08-24, exp 01b, first run.

Fix pattern for non-REST Iceberg catalogs on this stack: set `<catalog>.client.region=us-east-1` (keep s3.endpoint / s3.access-key-id / s3.secret-access-key / s3.path-style-access — those ARE real S3FileIOProperties keys). Working example: experiments/01b-catalog-snapshot-comparison/run_experiment.py hadoop_setup().

## Related
- [[kb-0043]] — Iceberg curriculum; exp 01b (catalog snapshot comparison) hit this while wiring the Hadoop catalog
- [[kb-0007]] — e2e-data-ai-project map (lakehouse_catalog.py lives there; it still uses the bogus s3.region — harmless for REST, would break a hadoop-type catalog)
