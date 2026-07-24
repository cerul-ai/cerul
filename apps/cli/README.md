# Cerul CLI

The CLI is a thin client for the generated Cerul contract. It does not contain
a planner or runtime implementation.

```sh
export CERUL_API_KEY=...
cerul capabilities
cerul search "grounded evidence" --library-id library_demo1
cerul ask "summarize the evidence" --library-id library_demo1
cerul agent-session get asess_demo1
cerul jobs get job_demo1
cerul export --library-id library_demo1 --input-json '{"selections":[]}'
```

For local use:

```sh
export CERUL_BASE_URL=http://127.0.0.1:23785/v1
export CERUL_INSTALLATION_TOKEN=...
cerul search "local evidence" --execution-policy local-only
```

Every command renders the contract response as JSON. Mutating retries use an
`Idempotency-Key`. Package publication and binary release are handled only
after an authorized release gate.
