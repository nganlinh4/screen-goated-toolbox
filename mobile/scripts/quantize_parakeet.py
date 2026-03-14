"""
Quantize Parakeet EOU 120M ONNX models to INT8 (dynamic quantization).
Uses large model loading for >2GB protobuf limit.
"""

import os
import sys
import onnx
from onnxruntime.quantization import quantize_dynamic, QuantType

MODEL_DIR = os.path.join(
    os.environ.get("APPDATA", ""),
    "screen-goated-toolbox", "models", "parakeet"
)

def quantize_model(input_name, output_name):
    input_path = os.path.join(MODEL_DIR, input_name)
    output_path = os.path.join(MODEL_DIR, output_name)

    if not os.path.exists(input_path):
        print(f"ERROR: {input_path} not found")
        return False

    if os.path.exists(output_path):
        print(f"SKIP: {output_path} already exists")
        return True

    input_size = os.path.getsize(input_path) / (1024 * 1024)
    print(f"Quantizing {input_name} ({input_size:.1f} MB) -> {output_name}...")

    # Use use_external_data_format for large models
    quantize_dynamic(
        model_input=input_path,
        model_output=output_path,
        weight_type=QuantType.QInt8,
        per_channel=True,
        extra_options={"MatMulConstBOnly": True},
    )

    output_size = os.path.getsize(output_path) / (1024 * 1024)
    ratio = output_size / input_size * 100
    print(f"  Done: {input_size:.1f} MB -> {output_size:.1f} MB ({ratio:.0f}%)")
    return True

if __name__ == "__main__":
    # Increase protobuf size limit for large models
    os.environ["PROTOCOL_BUFFERS_PYTHON_IMPLEMENTATION"] = "python"

    print(f"Model dir: {MODEL_DIR}")
    ok1 = quantize_model("encoder.onnx", "encoder.int8.onnx")
    ok2 = quantize_model("decoder_joint.onnx", "decoder_joint.int8.onnx")
    if ok1 and ok2:
        print("\nAll done! INT8 models ready.")
    else:
        print("\nFailed.")
        sys.exit(1)
