# Provider availability monitoring

A scheduled probe that publishes which provider endpoints answer, answer
correctly, and with which reasoning control. It exists because those facts are
expensive to discover from a user's machine — seventy-five models times three
samples every two hours — and because they change: three NVIDIA endpoints
changed state within a single day during evaluation.

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
