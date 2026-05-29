# erpai-cli

> Natural-language CLI for ERP data. Invoices · payroll · inventory · 30+ business objects. Ask in English, get streamed answers.

`erpai` is the command-line client for the [ERP•AI](https://erp.ai) platform. Point it at your tenant, ask a question in plain English, and it translates, runs, and streams the answer back — no query language, no dashboards.

```sh
erpai chat "what's our AR aging over 60 days by customer?"
erpai chat "show last month's payroll run totals by department"
erpai chat "which POs are still open against vendor Acme?"
```

## Install

Signed binaries (macOS · Linux · Windows) ship from [`erphq/erpai-cli-releases`](https://github.com/erphq/erpai-cli-releases) via GitHub Releases. Install the latest:

```sh
curl -fsSL https://raw.githubusercontent.com/erphq/erpai-cli-releases/main/public/install.sh | sh
```

The installer platform-detects and drops the `erpai` binary on your `PATH`. See the [download page](https://github.com/erphq/erpai-cli-releases) for manual downloads and checksums.

## Getting started

1. **Install** — run the command above.
2. **Log in** — `erpai login` (authenticates against your ERP•AI account).
3. **Ask** — `erpai chat "<your question>"`, or drop into the interactive REPL with `erpai chat`.

## Requirements

- An [ERP•AI](https://erp.ai) account
- macOS (Apple Silicon / Intel), Linux (x64), or Windows (x64)

## Status

This repo is the public home for the CLI. Binaries are distributed through [`erpai-cli-releases`](https://github.com/erphq/erpai-cli-releases); release source is tracked there. Issues and feature requests are welcome here.

## Links

- [ERP•AI platform](https://erp.ai)
- [Releases + install](https://github.com/erphq/erpai-cli-releases)
- [Architecture map](https://github.com/erphq/MetaRepo) (org members)
