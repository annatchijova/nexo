# Web UI contract

## Purpose

`web/` is the TypeScript adapter for the NEXO HTTP API. It renders authorized
case state and deterministic evaluation explanations; it does not evaluate
actions, interpret evidence, call a model, or perform an external legal act.

## Supported slice

- Store the API base URL, bearer token, and current case id locally in the
  browser so a single-owner personal workspace can be resumed.
- Create a case and read its authorized graph-node summary with creation
  timestamps for the timeline.
- Submit plain-text evidence to the API's sandboxed ingestion route.
- Record a confirmed owner assertion as a distinct graph node.
- Request evaluation and render both actionable and non-actionable results.
- Restore the latest recorded evaluation when the saved case is re-opened.
- Show factual support node ids and captured legal-source citations whenever
  the API returns them.

The UI treats an extraction rejection or non-actionable evaluation as a
first-class visible state. It never turns missing support into an available
action and it never presents preparation as filing, sending, or submission.

## Trust boundary

The browser is user-controlled. It is not an authority boundary: the API
must authenticate the bearer token and enforce case ownership for every
request. Values returned by the API are escaped before the UI inserts them
into HTML. Citation links open in a separate context with `noreferrer`.

## Development

```text
npm install
npm run typecheck
npm run build
npm run dev
```

The Vite development proxy forwards `/v1` to `http://127.0.0.1:8080`. A
production deployment should serve the built assets from the same trusted
origin as the API or configure an explicit equivalent proxy.

## Non-goals for this slice

- Preparation/export UI is not implemented yet; the API capabilities exist but
  must be surfaced with explicit preview and download affordances.
- No artifact preview, OCR, parsing, or decompression occurs in the browser.
- No credential-management claim is made beyond the API contract's
  single-owner bearer-token limitation.
