#!/usr/bin/env python3
"""Generate the reviewed Windows model-delivery catalog.

The direct-file contracts below are immutable Hugging Face revisions with
exact SHA-256 values. Archive inventories come only from the deterministic
output of package_windows_models.py. This script performs no network access.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def hf_file(
    repo: str,
    revision: str,
    source_path: str,
    output_path: str,
    size: int,
    sha256: str,
) -> dict:
    return {
        "path": output_path,
        "url": f"https://huggingface.co/{repo}/resolve/{revision}/{source_path}",
        "sizeBytes": size,
        "sha256": sha256,
    }


def direct_model(
    component_id: str,
    version: str,
    files: list[dict],
    legacy_root: dict | None,
) -> dict:
    value = {
        "id": component_id,
        "version": version,
        "architecture": "any",
        "installedSizeBytes": sum(item["sizeBytes"] for item in files),
        "files": files,
    }
    if legacy_root is not None:
        value["legacyRoot"] = legacy_root
    return value


def qwen_model(large: bool) -> dict:
    repo = "Qwen/Qwen3-ASR-1.7B" if large else "Qwen/Qwen3-ASR-0.6B"
    revision = (
        "7278e1e70fe206f11671096ffdd38061171dd6e5"
        if large
        else "5eb144179a02acc5e5ba31e748d22b0cf3e303b0"
    )
    common = [
        ("generation_config.json", 142, "1da527824d81e07118facff437e03f2e24a23311e3bdeb2368973fe77e5f275c"),
        ("merges.txt", 1_671_853, "8831e4f1a044471340f7c0a83d7bd71306a5b867e95fd870f74d0c5308a904d5"),
        ("preprocessor_config.json", 330, "45e120a4eda2c20c5d7f2ea9354e63536bf35e27aa573fb7cdf78017b378770d"),
        ("tokenizer_config.json", 12_487, "4942d005604266809309cabc9f4e9cb89ce855d59b14681fdc0e1cc62ea26c4c"),
        ("vocab.json", 2_776_833, "ca10d7e9fb3ed18575dd1e277a2579c16d108e32f27439684afa0e10b1440910"),
    ]
    if large:
        values = [
            ("config.json", 6_194, "2e74a751548b8ad7d7526d29365ad8144c345d8b412b1152d25dc6698452712f"),
            *common,
            ("model-00001-of-00002.safetensors", 4_220_320_824, "a4cd1f1a04d90b757dc7f7dd26254e69a013b19e80efe590a83c6a3bde8608d6"),
            ("model-00002-of-00002.safetensors", 478_200_688, "6e0b9d9e09e2e0238e7ef3cc8a484ab387e91b90f1900bedf88bc92d7929ccfc"),
            ("model.safetensors.index.json", 64_821, "f994739fe38e5210b9e3e8ce6c6307315e2ceac3cb630e7b7414d69dce520f60"),
        ]
        component_id = "qwen3-asr-1-7b-model"
        legacy = "qwen3_asr_1_7b"
    else:
        values = [
            ("config.json", 6_193, "76d3ae4601ce939830b2517f4a6cadb86cc51316c3900af6b020b051c21a478c"),
            *common,
            ("model.safetensors", 1_876_091_704, "79d6cbd4c98c7bbffe9db2edac07f56cd6637d0d5944b27f6c2b8353840323ea"),
        ]
        component_id = "qwen3-asr-0-6b-model"
        legacy = "qwen3_asr_0_6b"
    files = [hf_file(repo, revision, path, path, size, sha) for path, size, sha in values]
    files.append(
        hf_file(
            repo,
            revision,
            "README.md",
            "notices/MODEL-CARD.md",
            57_456,
            "5058416891bc47a2051557765997e8c42f8eb78a0e33c3e775bd17d4b0ba4d50",
        )
    )
    return direct_model(
        component_id,
        revision,
        files,
        {"kind": "roaming-models", "path": legacy},
    )


def magpie_model() -> dict:
    magpie_revision = "34d7e40da85cabc97f92198889b65cea27bc7fd1"
    codec_revision = "fc00890b604aa2de298d2641ffc6c5f6caf8c4d7"
    files = [
        hf_file(
            "nvidia/magpie_tts_multilingual_357m",
            magpie_revision,
            "magpie_tts_multilingual_357m.nemo",
            "magpie_tts_multilingual_357m.nemo",
            1_208_883_200,
            "3111c41d88de500dbc0cee70802c0ae7fb54915c46f29a2391a4510081f76a94",
        ),
        hf_file(
            "nvidia/nemo-nano-codec-22khz-1.89kbps-21.5fps",
            codec_revision,
            "nemo-nano-codec-22khz-1.89kbps-21.5fps.nemo",
            "nemo-nano-codec-22khz-1.89kbps-21.5fps.nemo",
            425_021_440,
            "28c2518de3e3d5a2c7d9bca40a7ebc0644eb76c60b890970365325bdd8e9f099",
        ),
        hf_file(
            "nvidia/magpie_tts_multilingual_357m",
            magpie_revision,
            "README.md",
            "notices/MAGPIE-MODEL-CARD.md",
            18_754,
            "133c8c472999baf99d2a4d78423a093158c4052b7089cdb0504f3773a4ed7eb1",
        ),
        hf_file(
            "nvidia/nemo-nano-codec-22khz-1.89kbps-21.5fps",
            codec_revision,
            "README.md",
            "notices/NANOCODEC-MODEL-CARD.md",
            13_074,
            "80c494428488c0c5823b52b0c87efed0f2f48d53087cb27e38b76e4aa8f0608e",
        ),
    ]
    return direct_model(
        "magpie-multilingual-357m-model",
        "34d7e40-fc00890",
        files,
        {"kind": "roaming-models", "path": "magpie_multilingual_357m"},
    )


def step_audio_model() -> dict:
    edit_repo = "stepfun-ai/Step-Audio-EditX-AWQ-4bit"
    edit_revision = "a3493ca914ef0dd3c265cf589b962e6213124958"
    tokenizer_repo = "stepfun-ai/Step-Audio-Tokenizer"
    tokenizer_revision = "af7e5a3ec06175a7facae9d4100073d6e4dbb36c"
    edit_values = [
        ("config.json", 1_412, "7f68d5b611d2eb22501773685a5822d3b185262874bf74cd9fffbc144c92df53"),
        ("configuration_step1.py", 2_087, "0bccd676d649f01a57d744c97a037b3cd8351077d29e0a61c6a973f8131cbafd"),
        ("generation_config.json", 132, "9e7f49baf57e36b8ac6600567c30794b977f80a3cf35665e2533e2d28e9d15fa"),
        ("model.safetensors", 2_502_127_032, "e262d33715b336a7b8a7f9e8f165da9c015775432b864a0fffa2226068996e45"),
        ("recipe.yaml", 1_353, "8503473c31c0ff3853c026cf871a624b91d3297d341aee2ad1deabdb91682bca"),
        ("tokenizer.model", 1_264_044, "25e122d9205d035033a9994c4d46a6a1b467a938654e4178fc0e5f4f5d610674"),
        ("tokenizer_config.json", 757, "f4867ace4775a7ccce881e4d3a47ad99d3e6e9c406e83f4632819dc44fa34303"),
        ("CosyVoice-300M-25Hz/FLOW_VERSION", 42, "4c5260393b4daf23759fa105c1059c7bd046cdebab63fd743c0ee3cd378ffa7d"),
        ("CosyVoice-300M-25Hz/campplus.onnx", 28_303_423, "a6ac6a63997761ae2997373e2ee1c47040854b4b759ea41ec48e4e42df0f4d73"),
        ("CosyVoice-300M-25Hz/cosyvoice.yaml", 1_980, "c117359097acb3d1345321c395abeb02071af9bb5bc0bf04caa84dedf77fa305"),
        ("CosyVoice-300M-25Hz/flow.pt", 615_274_252, "8ed68d0cf1bc13c86da5cb73241da37e8d7cebdfd13b92a620c6b3cb3aab872b"),
        ("CosyVoice-300M-25Hz/hift.pt", 117_228_443, "10bb4b495742ab00d92fd5ccb8d576164425d3a7d43a7f894ab0370d64e8a190"),
        ("CosyVoice-300M-25Hz/speech_tokenizer_v1.onnx", 522_625_011, "56285ddd4a83e883ee0cb9f8d69c1089b53a94b1f78ff7e4a0224a27eb4cb486"),
    ]
    tokenizer_prefix = "dengcunqin/speech_paraformer-large_asr_nat-zh-cantonese-en-16k-vocab8501-online"
    tokenizer_values = [
        ("linguistic_tokenizer.npy", 2_097_280, "d3f240ec9a524644ffd3790b6c568dd1554adfc366e76ddc4d6309a121a05fcc"),
        ("speech_tokenizer_v1.onnx", 522_625_011, "56285ddd4a83e883ee0cb9f8d69c1089b53a94b1f78ff7e4a0224a27eb4cb486"),
        (f"{tokenizer_prefix}/am.mvn", 11_203, "29b3c740a2c0cfc6b308126d31d7f265fa2be74f3bb095cd2f143ea970896ae5"),
        (f"{tokenizer_prefix}/config.yaml", 2_940, "35e6bf41f8c7eaf9a0f787af7fdc8fc5ed75fa8009ade7d3c2f3ef5bce20c648"),
        (f"{tokenizer_prefix}/configuration.json", 482, "6e8e8343128e006aa9bad7ad1b307c903c243a96dd2857566723cb2e922d1587"),
        (f"{tokenizer_prefix}/model.pt", 881_120_125, "5d8c231bf4c6c2643577b87c7eeb4c5f1a8175996540c41aa87b06acf55a6d0c"),
        (f"{tokenizer_prefix}/seg_dict", 8_287_834, "59a2ef803a3f1648ad03a2e1480db1c1ee0c0d7dc4ef4dbd16cea33944329022"),
        (f"{tokenizer_prefix}/tokens.json", 99_450, "f313c92ee2ba8fab6ec10cafac4b63a3c183425ca8b4771c57f68637e1d9bb64"),
        (f"{tokenizer_prefix}/tokens.txt", 39_940, "3b36439f2eb94b20930094d5a45a22db9f2ed1907bc7a2535e793d978b000406"),
    ]
    files = [
        hf_file(edit_repo, edit_revision, path, f"editx_awq/{path}", size, sha)
        for path, size, sha in edit_values
    ]
    files.extend(
        hf_file(tokenizer_repo, tokenizer_revision, path, f"tokenizer/{path}", size, sha)
        for path, size, sha in tokenizer_values
    )
    files.extend(
        [
            hf_file(edit_repo, edit_revision, "README.md", "notices/EDITX-MODEL-CARD.md", 37_443, "174fa210641656adfe5385b95f0a10123724f5458312f32b8fb36017c3baa451"),
            hf_file(tokenizer_repo, tokenizer_revision, "README.md", "notices/TOKENIZER-MODEL-CARD.md", 928, "87c609ababfbc0469ceb84f0fdf4af1c1379fa8d4c8c709ff3f9e43688ab71bf"),
            hf_file(tokenizer_repo, tokenizer_revision, f"{tokenizer_prefix}/README.md", "notices/PARAFORMER-MODEL-CARD.md", 4_736, "cc005b98a73a44faa579ee2bdff88bf3acce6b492d8716f024a65ceeb747b1b0"),
        ]
    )
    return direct_model(
        "step-audio-editx-model",
        "a3493ca-af7e5a3",
        files,
        {"kind": "local-models", "path": "step_audio_editx"},
    )


def vieneu_model() -> dict:
    repo = "pnnbao-ump/VieNeu-TTS-v2-Turbo"
    revision = "afe400abff18c00b52b246bb4d21f02a86855eb7"
    values = [
        ("added_tokens.json", 378_493, "cc57d2dc403cbe6565a873467a8dc250427ba49d0ca5d38f08af073f7c78412e"),
        ("config.json", 1_086, "accbd5ca76f270c7a36042475ef5a700389ae95096164d0abb1fee3f11978335"),
        ("generation_config.json", 125, "395e5c62d342ea4982dea39e98bd04511c99ea2d3232cdbb74d0e423c997b2c9"),
        ("merges.txt", 758, "c795c8cf497405235c3c03f1887024a933cd0bc0461a8731870bedbf6d84635d"),
        ("model.safetensors", 223_215_384, "507b50b6bf8bae54022164aad75c9ad4bb651edf40445497dbd38c201cbb837d"),
        ("special_tokens_map.json", 441, "dfbec48c0c6175cddb4e6a2d937c64bcbba3c82cedaddb51cd151124625c37ba"),
        ("tokenizer.json", 2_645_150, "054df1b05ead2232e4d64f34e20fa7b6c16c764060e897ea30058964bf8d1eec"),
        ("tokenizer_config.json", 2_494_338, "7909858e5c1d8e961c3f525bfbcdd1d64cda24e89c1c5e0f44be8070ad56a4e1"),
        ("vocab.json", 3_442, "1889eb451c473b0c856ff1c546c7b5b97a54163693fb28d14493b485352a9467"),
        ("voices.json", 15_596, "303bdfdb980a2e7ff227179f4b27839584ffc03c2040ccabc4f9a66ad0751e0c"),
    ]
    files = [
        hf_file(repo, revision, path, f"backbone/{path}", size, sha)
        for path, size, sha in values
    ]
    codec_repo = "pnnbao-ump/VieNeu-Codec"
    codec_revision = "eee9889a4176270272a07395c6540e06f9312184"
    files.extend(
        [
            hf_file(codec_repo, codec_revision, "vieneu_decoder.onnx", "codec/vieneu_decoder.onnx", 345_442_987, "2252107f20222cd321154db429a0eb3f81e4e82b7a8bcb8872adb157ed3605d2"),
            hf_file(codec_repo, codec_revision, "vieneu_encoder.onnx", "codec/vieneu_encoder.onnx", 117_338_685, "ed11494fa09427bf4ce92652c8e35ece9dd9d45042a99e0cf6ea41fd9cf7e86f"),
            hf_file(repo, revision, "README.md", "notices/BACKBONE-MODEL-CARD.md", 6_024, "5af5ca3d9e71fd8d0aed920b0dbfe743b0737d050b6d88fb4bd89c9729772f19"),
            hf_file(codec_repo, codec_revision, "README.md", "notices/CODEC-MODEL-CARD.md", 1_502, "3f60f40f6fb6d9338da49b338455bc1489869f4b043e68cb04c6c899c1bdd495"),
        ]
    )
    return direct_model("vieneu-v2-turbo-model", "afe400a-eee9889", files, None)


def generate_delivery(packages: dict) -> dict:
    if packages.get("schemaVersion") != 1:
        raise ValueError("unsupported package-manifest schema")
    archive_models = {item["id"]: item for item in packages["models"]}
    for component_id, legacy_path in (
        ("kokoro-82m-v1-model", "kokoro_v1"),
        ("supertonic-3-model", "supertonic_3"),
    ):
        archive_models[component_id]["legacyRoot"] = {
            "kind": "roaming-models",
            "path": legacy_path,
        }
    models = [
        qwen_model(False),
        qwen_model(True),
        step_audio_model(),
        magpie_model(),
        archive_models.pop("kokoro-82m-v1-model"),
        archive_models.pop("supertonic-3-model"),
        vieneu_model(),
    ]
    if archive_models:
        raise ValueError(f"unexpected package models: {sorted(archive_models)}")
    return {"schemaVersion": 1, "models": models}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    packages = json.loads(args.package_manifest.read_text(encoding="utf-8"))
    output = generate_delivery(packages)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(args.output)
    for model in output["models"]:
        print(f"{model['id']}: {model['installedSizeBytes']} bytes, {len(model['files'])} files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
