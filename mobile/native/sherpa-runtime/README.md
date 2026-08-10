# Reduced Sherpa Android runtime

This directory owns the reproducible build and delivery contract for the
arm64 Android Zipformer runtime. The app uses only streaming ordinary
transducers whose embedded model type is `zipformer` or `zipformer2`, greedy
search, and the JNI surface listed in `build-contract.json`.

The checked-in runtime is built from sherpa-onnx v1.12.35 and ONNX Runtime
v1.23.2. ONNX Runtime remains able to load normal ONNX files; only its kernel
registry is reduced. The operator list is generated after ONNX Runtime `Fixed`
graph optimization so optimizer-created Microsoft-domain kernels are retained.
An operator list taken directly from the source graphs is unsafe and previously
failed at model initialization.

From the repository root on Windows:

```powershell
mobile/scripts/build-sherpa-runtime.ps1 -WorkDir C:\build\sgt-sherpa
cd mobile
.\gradlew.bat verifyNativeRuntimeArchives
```

The build script checks source commits, applies the owned narrowing patch,
builds with NDK 27, strips the ELF, verifies the exact JNI/DT_NEEDED contract,
and writes a timestamp-normalized ZIP. Run the Android contract task after
updating the artifact identity in the shared native runtime fixture.

Real-device acceptance for a new build must initialize and decode at least one
catalog Zipformer v1 model with BPE and one Zipformer2 model. The artifact
currently checked in passed that test on arm64 with the pinned Korean and
English catalog models.

License texts and ONNX Runtime's complete third-party notices are packaged in
the same Play feature as the library and in the Full application that can
download it. `assets/third_party/sherpa-runtime/NOTICE.txt` records the
provenance of this custom build.
