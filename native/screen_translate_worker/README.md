# Incremental Screen Translate Worker

Native geometry and recognition orchestration for Screen Translate. The worker
uses protocol `stream` from `screen_text_detector_protocol`; the prior worker
and its immutable package remain the rollback implementation, not a per-request
fallback. The host supplies and leases every runtime/model path explicitly.

One capture produces geometry, independently completed readings, then a terminal
event. Cancellation interrupts pending recognition without assigning late output
to a newer capture. ONNX Runtime DirectML owns localization and batched compact
CTC recognition. Every detected region is submitted; there is no text-size or
confidence admission gate and no generative-reader fallback. A visual script
classifier normalizes foreground/background polarity independently of the reader
pixels and selects a reader per line and at most one alternative. Nearby script
evidence can corroborate an alternative alphabet; the general reading is retained
unless the alternative supplies that missing alphabet or recovers an empty result.
Repeated alternative script evidence can also recover empty or symbol-only output;
ordinary words and numbers do not trigger this recovery by themselves.
All readers warm before readiness. Width-sorted batches
retain region identities. Worker startup does not select delivery sources.

Advisory document layout runs on DirectML alongside localization. Geometry carries
both the complete OCR inventory and separate layout regions; layout never filters
text. The host builds immutable logical units from pixel geometry, visual style,
separators and advisory boundaries. A unit waits only for its own readings, is
translated once, and owns the same area during fitting and independent reveal.
An unreadable member preserves its unit's original pixels. Sessions are prepared
on feature use, never during ordinary application startup.

From the repository root, use `cargo test --manifest-path
native/screen_translate_worker/Cargo.toml` and the corresponding
`cargo clippy --all-targets -- -D warnings` with the managed development target
directory. Packaging and staging follow `docs/COMPONENT_DELIVERY.md`.

`scripts/package_screen_translate_worker.py --help` lists the explicit package
inputs. `package-inputs.json` pins their upstream bytes and owns the runtime file
inventory used by both packaging and the host build validator. Output must live
outside the repository. The packager verifies inputs, includes dependency
notices, and checks two deterministic archives before producing a staging
descriptor; it does not change the production delivery contract.
`--input-root` contains the inventory paths from `package-inputs.json`. Packaging
uses `onnx==1.19.1` for the hash-checked top-candidate output transformation.
`readers.json` owns script routing and dictionary paths within that inventory.
The package contains the worker and pinned detector/recognizer models. The host
leases the shared ONNX runtime separately; no language-model server is shipped.
