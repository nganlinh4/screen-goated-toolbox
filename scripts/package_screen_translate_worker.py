#!/usr/bin/env python3
"""Package the pinned native incremental Screen Translate component."""
from __future__ import annotations

import argparse
import json
import shutil
import zipfile
from pathlib import Path

import package_screen_text_detector as shared


def verify(path: Path, digest: str) -> Path:
    if not path.is_file() or path.is_symlink() or shared.sha256(path) != digest:
        raise ValueError(f"Package input differs from pinned bytes: {path}")
    return path


def archive(path: Path, files: list[tuple[str, Path]]) -> None:
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as target:
        for relative, source in sorted(files):
            info=zipfile.ZipInfo(relative, shared.FIXED_TIMESTAMP)
            info.compress_type=zipfile.ZIP_STORED if relative.endswith(".gguf") else zipfile.ZIP_DEFLATED
            info._compresslevel=1
            info.create_system=3
            info.external_attr=(0o100755 if relative.endswith(".exe") else 0o100644)<<16
            with source.open("rb") as reader, target.open(info,"w") as writer:
                shutil.copyfileobj(reader,writer,1024*1024)


def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ["worker", "input-root", "output-dir"]:
        parser.add_argument("--"+name,required=True,type=Path)
    args=parser.parse_args()
    repo=Path(__file__).resolve().parents[1]
    output=args.output_dir.resolve()
    if output.is_relative_to(repo):
        raise ValueError("Package output must be outside the repository")
    output.mkdir(parents=True,exist_ok=True)
    manifest=repo/"native/screen_translate_worker/Cargo.toml"
    inputs_path=manifest.with_name("package-inputs.json")
    inputs=json.loads(inputs_path.read_text(encoding="utf-8"))
    version=shared.worker_version(manifest)
    shared.validate_x64_pe(args.worker)
    files=[("bin/x64/sgt-screen-text-detector-worker.exe",args.worker)]
    for section in ["models", "licenses"]:
        for record in inputs[section]:
            relative=Path(record["path"])
            if relative.is_absolute() or ".." in relative.parts:
                raise ValueError("Package input must be relative")
            source=verify(args.input_root/relative,record["sha256"])
            if transformation := record.get("transformation"):
                if transformation != "sorted-top3":
                    raise ValueError("Unknown model transformation")
                transformed=output/(record["packagedSha256"]+".onnx")
                if shared.compact_recognizer_model(source,transformed) != record["packagedSha256"]:
                    raise ValueError("Transformed model differs from reviewed bytes")
                source=transformed
            files.append((record["path"],source))
    shared.FIRST_PARTY.add("sgt-screen-translate-worker")
    inventory,notices=shared.write_rust_licenses(repo,manifest,output)
    files.extend([("licenses/THIRD-PARTY-LICENSES.json",inventory),
                  ("licenses/THIRD-PARTY-NOTICES.txt",notices),
                  ("licenses/PACKAGE-INPUTS.json",inputs_path),
                  ("models/pp-ocr-screen-text/readers.json",manifest.with_name("readers.json"))])
    records=[{"path":relative,"sizeBytes":path.stat().st_size,"sha256":shared.sha256(path)} for relative,path in sorted(files)]
    first=output/"screen-text-detector.candidate.zip"
    second=output/"screen-text-detector.verify.zip"
    archive(first,files);digest=shared.sha256(first)
    archive(second,files)
    if shared.sha256(second)!=digest:
        raise ValueError("Component package is not deterministic")
    second.unlink()
    name=f"screen-text-detector-{version}-{digest[:16]}.zip"
    destination=output/name
    if destination.exists():
        verify(destination,digest);first.unlink()
    else:first.rename(destination)
    component={"id":"screen-text-detector","version":version,"asset":name,
               "assetPath":str(destination),"sizeBytes":destination.stat().st_size,
               "sha256":digest,"unpackedSizeBytes":sum(r["sizeBytes"] for r in records),"files":records}
    if component["sizeBytes"]>2_000_000_000:
        raise ValueError("Component exceeds the release asset boundary")
    descriptor=output/"screen_text_detector.packages.json"
    descriptor.write_text(json.dumps({"schemaVersion":1,"architecture":"x64","component":component},indent=2)+"\n",encoding="utf-8")
    print(descriptor)


if __name__=="__main__":
    main()
