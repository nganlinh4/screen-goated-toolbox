# Provider availability monitoring

A scheduled probe that publishes which provider endpoints answer, which general
modality they accept, and with which reasoning control. It exists because those facts are
expensive to discover from a user's machine — seventy-five models times three
samples every two hours — and because they change: three NVIDIA endpoints
changed state within a single day during evaluation.

## Why preset output never gates a whole catalog

The monitor owns operational evidence only: replies, latency, accepted modality,
and request control. It does not turn one preset's expected output into a verdict
on a general Text-to-Text or Image-to-Text model.

One controlled sample can establish whether an endpoint answers. It cannot
establish universal quality: translation, extraction, rewriting, summarization,
reasoning, and custom instructions fail in different ways. The same applies to
images: accepting an image is live capability evidence, while OCR accuracy is
only one use case.

So the rule is: **live monitoring gates operational health; broad, reviewed
catalog benchmarks own durable quality.** Samples reported from a real preset may
be added to that broad evaluation, but never become a preset-specific live ban.

The consequences of being wrong are not symmetric either, which settles where the
risk belongs:

- a **dead** endpoint is cheap, and the retry chain already handles it;
- a **wrong** endpoint is expensive, but a narrow automatic check can be wrong
  about the endpoint's other uses; quality changes therefore require broad
  benchmark evidence and review;
- a model that repeatedly stops answering or exceeds the usable latency ceiling
  stops being offered automatically.

Probe prompts stay small and varied only to avoid measuring one request shape.
Their content is not scored by the availability publisher.

## What the feed is and is not

`nvidia-availability.json` carries availability, accepted general modality and
the working reasoning control. Those are live properties of the endpoint.

It also carries `p50_ms`, and that number is **a ranking hint only**. A GitHub
runner measures from one datacenter; the user is on their own network, often in
another region. The client combines durable catalog quality and latency evidence
with the feed's fresh measurements. The feed answers "is this endpoint usable",
not "is it universally fastest".

## Why the reasoning control is per model

Applying one control across a provider fabricates results. During evaluation a
blanket `reasoning_effort` rejected five endpoints that work, and sending it to an
endpoint that does not accept it turned a healthy model into HTTP 500. Community
leaderboards reproduce the same error, listing at 0% a model that answers three of
three when asked correctly.

So each model's control is discovered from an ordered ladder, cached in
`nvidia-history.json`, and re-discovered only when a model stops answering.

## Why eligibility is hysteretic

A single run never promotes or demotes anything. A model becomes eligible after
`RUNS_TO_PROMOTE` consecutive healthy runs and loses it after `RUNS_TO_DEMOTE`
consecutive failures. Without that, a provider whose endpoints flap would rewrite
the catalog every two hours.

## Where the data lives

The published feed and its rolling history are on the `monitoring-feed` branch,
not here. A job that commits every two hours and a human pushing to the same
branch collide constantly: in one day the feed produced eleven of twenty-nine
commits on `main` and forced a merge on every push. This directory keeps only the
public key and this document, both of which change rarely and belong with the
code that reads them.

## One-time setup

The workflow needs two repository secrets. Generate a signing key that is **not**
the update-catalog key: that one signs runtime bundles and is used by an attended
workflow, while this runs unattended every two hours and only influences model
routing. They must not share a key.

```bash
python - <<'PY'
import base64
from ecdsa import NIST256p, SigningKey
key = SigningKey.generate(curve=NIST256p)
print("secret SGT_MONITORING_P256_PRIVATE_KEY_PEM_BASE64:")
print(base64.b64encode(key.to_pem()).decode())
print("\nwrite this to monitoring/monitoring-p256-public-key.hex:")
print(key.get_verifying_key().to_string().hex())
PY
```

1. Add `SGT_MONITORING_P256_PRIVATE_KEY_PEM_BASE64` as a repository secret.
2. Add `NVIDIA_API_KEY` as a repository secret.
3. Write the printed public key to `monitoring/monitoring-p256-public-key.hex` and
   commit it. The signing step refuses to run if the key does not match this file.

## Running it by hand

```bash
NVIDIA_API_KEY=... python scripts/monitor_nvidia_models.py \
  --models nvidia/nemotron-3.5-lightning-30b-a3b,openai/gpt-oss-20b \
  --history monitoring/nvidia-history.json \
  --output monitoring/nvidia-availability.json
```

`--models` probes a named subset and `--limit` takes the first N, both intended
for checking a change without spending a full cycle.

## What the catalog owns, and what the feed owns

The two are not competing registries. They answer different questions, and the
split follows from which one can still be true a month after a build is cut.

The **catalog** owns *curated product decisions*: which models lead a chain, what
they are called in each language, quotas, modality, and the defaults. These are
judgements someone made, they change when a human changes them, and compiling
them into the binary is correct.

The **feed** owns *live operational facts*: whether an endpoint answers, how fast,
which reasoning control it currently accepts, and whether it is eligible at all.
Every one of these can change without anybody editing anything, so freezing them
at build time guarantees they eventually lie. Two consequences:

- The published reasoning control **overrides** the catalog policy. Both describe
  how to ask an endpoint to stop thinking; only one is re-measured every two
  hours, and sending a control an endpoint no longer accepts turns a healthy
  model into HTTP 500.
- A model the feed offers but the catalog has never heard of is **routable
  anyway**, as `ModelSource::Discovered` with an id derived from its endpoint
  name. This is what stops the product's freshness being tied to its release
  cadence: a model the monitor finds on Tuesday is usable on Tuesday.

Discovery adds and never redefines identity or presentation. Manual priority
order remains exact while Live is off. Turning Live on is an explicit opt-in to
re-rank every currently offered row below the protected head from fresh feed
signals while preserving the relative order of non-live authored rows.

Reasoning controls use the versioned `controlVersion` contract. Labels are a
closed enum in each client version; changing a label's wire meaning requires a
new contract version. System-message controls are merged into an existing system
instruction instead of creating competing system roles.

Eligibility also carries `availabilityGateVersion`. A client rejects feeds
produced by obsolete operational semantics even when their signature is valid.
Changing a benchmark task never resets endpoint availability.

## Client contract

The implemented contract is:

- the feed may **offer** models, inserted as `ModelSource::Discovered`;
- dedicated capabilities are omitted from this generic availability feed and
  never enter a text or vision chain merely because the endpoint is implemented
  as an LLM. A translation-only endpoint is governed like any other dedicated
  translator, not like a general text model;
- eligible offers are interleaved by speed-forward quality-adjusted latency: one
  higher catalog quality tier may justify up to roughly 1.5x the latency, while
  non-live configured fallbacks retain their relative order;
- at most five adaptive offers enter one chain, preserving provider diversity
  and limiting the blast radius of one live provider outage;
- adaptive offers render as normal editable rows. Non-live edits preserve Live;
  moving one pins only that row, removing one records a chain-local exclusion,
  and changing one excludes its old identity while pinning a live replacement.
  These row-level overrides preserve Live, so later successful refreshes may
  update every other live row from the newest measured latency. Restoring chain
  defaults clears the pins and exclusions. A manual edit that leaves no live-feed
  row in the chain turns Live off;
- image `10` and text `12` are shipped-default preparation targets only; user
  chains are never truncated by the editor, config migration, or runtime;
- **position 0 is never remote-controlled**, because it is tied to
  `default_text_model_id` and `default_image_model_id` and carries every request
  before any fallback exists;
- the signature must verify against the tracked public key before any of that.
