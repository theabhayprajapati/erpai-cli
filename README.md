# erpai — the ERP•AI command-line client

`erpai` is how coding agents (and people) work with [ERP•AI](https://apps.erp.ai) from a terminal: apps, tables, columns, records, SQL, workflows, views, forms, documents, roles, pages, widgets, the public catalog, and attachments — over the platform's public API, with one output contract and a safety model designed for agents.

## Install

**With the ERP•AI agent plugin (recommended)** — the plugin pins and bundles the CLI:

```
/plugin marketplace add erphq/agent-plugins
/plugin install erpai@erpai-plugins
```

**Standalone** — download a release binary for your platform from
[`erphq/erpai-cli-releases`](https://github.com/erphq/erpai-cli-releases/releases/latest), verify it against `sha256.txt`, and put it on your `PATH`.

## Sign in

```
erpai login                 # opens the browser once; you pick the org and the apps this key may touch
erpai login --api-key …     # store an existing key instead
erpai whoami                # who you are, which org, which apps the key can reach
erpai doctor                # profile, base URL, credential — exit code is the first failing check
```

`erpai login` mints an API key that is **restricted to the apps you chose** and to least-privilege scopes; `--read-only` narrows it further, `--all-apps` widens it (and asks). Credentials live in `~/.config/erpai/profiles/<profile>.json` (mode 0600). One profile = one environment: `--profile` selects it; the base URL is set at login and never by a per-command flag.

## Contract

- **stdout is JSON, always.** `{"data": …}` for one item; `{"data": [...], "page": {"no","size","total"}}` for lists. Writes add `"context": {"profile","org","app"}` so the transcript shows where the write went.
- **stderr is JSON, only on failure.** `{"error": {"code","message","hint"?,"requestId"?}}`.
- **Exit codes are categorical.** `0` ok · `1` internal · `2` validation · `3` auth · `4` forbidden · `5` network · `6` API error · `7` not found.
- `erpai <group> <command> --help` documents each command's output shape and errors. Read it instead of memorising flags.

## Commands

| Group | Commands |
|---|---|
| `apps` | `list`, `get` |
| `tables` | `list`, `get`, `create`, `update`, `delete`, `actions list\|create\|delete` |
| `columns` | `list`, `add` (object or array), `update`, `delete` |
| `records` | `query`, `count`, `aggregate`, `get`, `get-many`, `create`, `bulk-create`, `update`, `bulk-update`, `delete`, `bulk-delete`, `update-by-filter`, `delete-by-filter` |
| `sql` | `schema`, `run`, `generate` |
| `workflows` | `list`, `get`, `create`, `update`, `patch-node`, `rename`, `delete`, `activate`, `deactivate`, `execute`, `test-node`, `executions`, `execution`, `execution-stop`, `execution-retry`, `run-summary`, `run-node`, `nodes list\|schema\|options`, `credentials …` |
| `layouts` · `forms` | saved views · entry forms |
| `documents` | documents and folders |
| `roles` | roles, users, assign/remove, invitations |
| `pages` · `widgets` | custom pages, home config, table insight widgets |
| `catalog` | `list`, `get`, `activate` (the customer journey), `install`, `publisher overview\|terms\|accept-terms\|profile\|set-profile\|claim`, `source-publication`, `source-draft`, `draft`, `preview`, `publish`, `unpublish` |
| `attachments` | `upload` (returns a ready file-cell value), `download-url` |
| `api` | `get\|post\|put\|patch\|delete <path>` for public endpoints without a command (`--query`, `--body`/`--file`, `--header`); writes are gated like any other |
| `login` · `logout` · `whoami` · `doctor` · `update --check` · `settings` | lifecycle |

Global flags: `--app <id>` (or `ERPAI_APP_ID`, or a `.erpai/app` file), `--profile`, `--format json|table`, `--yes`, `--dry-run`.

## Safety model

- **Explicit target.** App-scoped commands need `--app`; there is no "last used app".
- **Pre-flight.** If the key is restricted to certain apps, a different `--app` fails with exit `4` *before* any request.
- **Destructive verbs ask.** `delete`, `bulk-delete`, `delete-by-filter`, `update-by-filter`, `tables delete`, `columns delete`, `workflows delete`, role changes, invitations, `catalog publish|unpublish|activate|install`, the permanent publisher steps, … require `--yes` or a typed confirmation. Filter-wide writes show the matching count first and refuse an empty filter unless `--all-rows --yes`.
- **`--dry-run`** on every mutating command validates and prints the request plan without sending it.
- **Only the public API.** The client can build paths under `/v1/` and `/open/v1/` only.

## Building from source

```
git clone https://github.com/erphq/erpai-cli && cd erpai-cli
cargo build --release          # target/release/erpai
cargo test                     # unit, contract, and safety suites (mock server; no network)
```

Requires Rust 1.85+. Releases are built by CI for macOS (arm64, x64), Linux (x64, arm64), and Windows (x64) and published to `erphq/erpai-cli-releases` with `sha256.txt`.
