# NEXO — Devpost form text (paste-ready, English)

This file is written to match Devpost's standard submission fields. Copy
each section into its corresponding box. Keep `devpost-submission.md` as
the full reference draft (Spanish, more detail, TODO checklist).

---

## Tagline / one-line summary

NEXO turns scattered messages, emails, and documents about a digital
rights violation into a case you can actually show someone — without
pretending to be your lawyer, and without ever sending anything on its
own.

---

## Inspiration

Every person who's gone through digital harassment, a privacy violation,
or had their data misused knows the same feeling: the proof exists, but
it's scattered across screenshots, chats, and half-remembered dates, and
the tools that promise to "know your rights" usually blur your own
account of events with the evidence and with a confident-sounding
conclusion — which is exactly the wrong thing to do to someone who is
scared, not a lawyer, and about to make a decision that matters.

We wanted a tool that never lies by omission and never lies by
overconfidence: one that keeps what you *have*, what you *said*, and what
it *concludes* as three separate, honestly labeled things — and that says
"there isn't enough here yet" instead of faking a result. Argentina's Ley
25.326 (personal data) and Ley 27.736 (digital violence) gave us real,
citable ground to build that on instead of a generic "know your rights"
gesture.

## What it does

You give NEXO what you have — a message, an `.eml`, a PDF, your own
account of what happened — and it builds a case graph that keeps
artifacts, your assertions, and derived inferences distinct. It evaluates
that graph against a versioned, source-cited legal bundle and returns one
of three honest outcomes: a supported path with the actual statute behind
it, a conditional result naming exactly what's missing, or a clear
negative with its precise cause. When there is a supported path, NEXO can
prepare the materials — a request, an evidence package, an export — but a
human always decides whether to send them.

Every artifact, export, and the exact legal bundle used gets a SHA-256
fingerprint, verifiable with a small standalone tool that doesn't need
NEXO's own server or database — so what you exported stays checkable even
if NEXO itself goes offline.

## How we built it

- **Rust workspace**, split into narrow crates: a pure domain model
  (`nexo-core`), two real Argentine policy bundles (`nexo-policy-ar`,
  `nexo-policy-ar-digital-violence`), transactions/persistence/auth
  (`nexo-app`), the only network-facing surface (`nexo-api`), a
  standalone integrity/verifier pair (`nexo-integrity`,
  `nexo-verifier`), and isolated per-format extractors
  (`nexo-extractor-plaintext`, `-eml`, `-pdf`) run inside
  `nexo-sandbox` so a hostile file can't take down the server.
- **PostgreSQL** for persistence, **Axum** for the HTTP API.
- **A bilingual TypeScript/Vite web client**, including a full
  browser-only DEMO (no token, no backend) that generates a real
  `.eml`, PDF, and SHA-256 manifest client-side.
- **SHA-256 + canonical serialization** for artifacts, exports, and the
  legal bundle in force, with an independent verifier that runs without
  the server.
- Deployed for real on a live EC2 instance behind Caddy — not just
  running locally.

## Challenges we ran into

- Making the evaluation **fail closed**: a missing legal requirement has
  to stay missing, never silently become "available" — including the
  negative and conditional outcomes, which took as much design work as
  the positive one.
- Isolating file extraction so a malformed `.eml` or PDF can't hang or
  crash the service — every extractor runs bounded, in its own sandboxed
  worker.
- Debugging a `401: expected Bearer token` flow and turning it into a
  clear, humane explanation in the UI instead of a dead end — a real
  person, not a developer, has to understand where their token comes
  from and why they don't get to invent one.
- Keeping the legal bundles honest: sourced from captured official
  statute text, versioned, and never paraphrased from memory.

## Accomplishments that we're proud of

- Two real, fully wired Argentine legal bundles (Ley 25.326, Ley 27.736)
  built from actual captured statute text, not a generic rules engine.
- A full evidence-to-export path confirmed by actually running the test
  suite against a live PostgreSQL database — not just reading the code.
- A public, no-token DEMO that lets anyone try the entire flow and
  download a real `.eml`, PDF, and SHA-256 manifest in under a minute.
- An independent verifier that checks NEXO's own exports without needing
  NEXO's server or database at all.
- A real deployment on EC2, not a purely local prototype.

## What we learned

That "honest and useless" and "helpful and overconfident" are both
failure modes — the hard, worthwhile work is the third option: helpful
*and* honest about exactly what it doesn't know yet. Keeping evidence,
personal statements, and inferences apart in the data model (not just in
the UI copy) is what made every other honesty guarantee — SHA-256
fingerprints, fail-closed evaluation, the negative result as a real
outcome — actually enforceable instead of aspirational.

## What's next for NEXO

- **Self-serve onboarding.** Today, getting a real workspace token means
  the administrator SSHes into the server and hands it to you by hand —
  that's real friction for the person this is actually for, and it's the
  next thing we want to build, not just document.
- A second, independently-built jurisdiction bundle for the United
  States, once the Argentine bundles complete a full red-team pass —
  never flattening two legal systems into one generic rule set.
- OCR support for scanned screenshots and image-only PDFs.
- Broader real-world backend testing beyond the browser DEMO.

---

## Built With (tags)

rust, axum, postgresql, typescript, vite, docker, sha-256, printpdf,
mermaid, claude, codex

## Try it

- Live demo (no token needed): https://nexo-web-sigma.vercel.app/demo
- Main site: https://nexo-web-sigma.vercel.app
- Repository: https://github.com/annatchijova/nexo
- Video: **PENDING — add link once recorded**

## AI usage disclosure (paste into the AI-tools question if asked separately)

Codex and Claude were used as assistance tools during development, code
review, debugging, and documentation. The core evaluation logic is
deterministic, based on versioned legal bundles, and does not depend on
any hidden model output — see the "How it's built" section and the
Technical README (docs/TECHNICAL_README.md) in the repository for the
full architecture and test evidence.
