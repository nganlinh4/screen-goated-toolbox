# Provider availability monitoring

A scheduled probe that publishes which provider endpoints answer, answer
correctly, and with which reasoning control. It exists because those facts are
expensive to discover from a user's machine — seventy-five models times three
samples every two hours — and because they change: three NVIDIA endpoints
changed state within a single day during evaluation.

## Where correctness is actually decided

Not here. The monitor answers "is this endpoint usable at all", periodically and
provider-wide. Whether a *particular answer* is correct is decided at runtime, in
`src/api/text/translate/fidelity.rs`, where the source and the reply are both in
hand.

That split exists because probe cases cannot keep up. There will be far more
models and far more real inputs than anyone can enumerate, and a case written
against yesterday's failure does not catch tomorrow's. Two failures made the point:
a reply that mixed Portuguese, Italian, Spanish and Tamil into a Vietnamese
translation, and one that fused a lone Hangul character onto a Vietnamese word.
Both passed a suite of short probes and both were caught structurally at runtime,
by a rule derived from the request rather than from a list.

The division is worth stating plainly:

- a **dead** endpoint is cheap, and the retry chain already handles it;
- a **wrong** endpoint is expensive, and only the runtime sees enough to judge it;
- a model that keeps producing wrong answers therefore stops being used without
  anyone configuring which models those are.

The probe cases that remain are coarse sanity, not a failure catalogue. They exist
to avoid publishing an endpoint that cannot translate at all, and they should stay
small.

## What the feed is and is not

`nvidia-availability.json` carries availability, correctness and the working
reasoning control. Those are properties of the endpoint and travel anywhere.

It also carries `p50_ms`, and that number is **a ranking hint only**. A GitHub
runner measures from one datacenter; the user is on their own network, often in
another region. The client already records real per-call latency through
`VisionCallTrace` and `latency::mark_window`, and should order by that. The feed
answers "is this endpoint usable", never "is it fastest for you".

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

## Client contract

Not yet implemented. When it is, the agreed shape is:

- the feed may **offer** models, inserted as `ModelSource::Discovered`;
- it may influence priority-chain order from **position 1 downward**;
- **position 0 is never remote-controlled**, because it is tied to
  `default_text_model_id` and `default_image_model_id` and carries every request
  before any fallback exists;
- the signature must verify against the tracked public key before any of that.
