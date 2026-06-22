#!/usr/bin/env python3
"""Export the MIT-licensed UI-DETR-1 detector (RF-DETR-M, Apache base) to ONNX for
the Screen Goated Toolbox computer-control local detector.

This is a ONE-TIME step run on a machine with Python + a torch install. It is NOT
part of the app build. The app loads the resulting .onnx via `ort` at runtime.

    pip install rfdetr huggingface_hub onnx
    python scripts/export_ui_detr.py

Then the .onnx is copied to:
    %APPDATA%/screen-goated-toolbox/models/ui-detector/ui-detr-1.onnx

IMPORTANT: RES below MUST equal RES in
    src/overlay/computer_control/detector.rs
UI-DETR-1 (RF-DETR-M, patch-16) requires a resolution divisible by 32; 1024 is a
good speed/accuracy balance (the model trained at 1600). Smaller = faster, lower
recall on tiny elements.

Licensing: UI-DETR-1 is MIT; RF-DETR (Nano-Large) + DINOv2 backbone are Apache-2.0.
No AGPL. Confirm the exact checkpoint's license before shipping.
"""
import os
import shutil

# Keep in sync with detector.rs `RES` (must be divisible by 32 for UI-DETR-1).
RES = 1024


def main() -> None:
    from huggingface_hub import hf_hub_download

    # 1) Fetch the UI-DETR-1 weights (RF-DETR-M finetune). Adjust `filename` if the
    #    repo names it differently (check the "Files" tab on the HF model page).
    weights = hf_hub_download(repo_id="racineai/UI-DETR-1", filename="model.pth")
    print(f"weights: {weights}")

    # 2) Load via the rfdetr package and export to ONNX. Class name may be
    #    RFDETRMedium / RFDETRBase depending on the rfdetr version — pick the one
    #    matching UI-DETR-1's base (RF-DETR-M).
    from rfdetr import RFDETRMedium

    model = RFDETRMedium(pretrain_weights=weights, resolution=RES)
    # `shape` is REQUIRED to pin a ÷32 input size; opset 17 matches the app's ort.
    # Verified I/O: input "input" [1,3,RES,RES]; outputs "dets" [1,300,4] (cxcywh,
    # 0..1) + "labels" [1,300,1] (logits). Writes output/rfdetr-medium.onnx.
    model.export(output_dir="output", shape=(RES, RES), opset_version=17)

    # 3) Place it where the app looks for it (asset name ui-detr-1.onnx).
    src = os.path.join("output", "rfdetr-medium.onnx")
    dst = os.path.join(
        os.environ["APPDATA"],
        "screen-goated-toolbox",
        "models",
        "ui-detector",
        "ui-detr-1.onnx",
    )
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    shutil.copy(src, dst)
    print(f"ONNX ready at: {dst}")
    print(f"Input resolution {RES}x{RES} — confirm RES matches detector.rs.")


if __name__ == "__main__":
    main()
