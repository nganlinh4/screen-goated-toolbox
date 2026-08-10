package dev.screengoated.toolbox.mobile.service.moonshine

data class MoonshineModelFile(
    override val name: String,
    override val byteCount: Long,
    override val sha256: String,
) : ManagedModelFile

data class MoonshineModelBundle(
    val asset: String,
    val byteCount: Long,
    val sha256: String,
    val downloadUrl: String,
)

private fun modelFile(name: String, byteCount: Long, sha256: String) =
    MoonshineModelFile(name, byteCount, sha256)

/**
 * Moonshine Voice model variants (English-only streaming).
 */
enum class MoonshineLanguage(
    val code: String,
    val displayName: String,
    val modelName: String,
    val moonshineArch: Int,
    val downloadBaseUrl: String,
    val modelFileContracts: List<MoonshineModelFile>,
) {
    ENGLISH_TINY(
        "en",
        "English (Tiny)",
        "tiny-streaming-en",
        2,
        "https://download.moonshine.ai/model/tiny-streaming-en/quantized",
        listOf(
            modelFile("adapter.ort", 1_319_440, "df13e655b29d279911fcb42d8b91b0e655b8fe32b7ba1f463ece663ce55ae6eb"),
            modelFile("cross_kv.ort", 1_264_384, "5acfca68f7bb068c68c1960b54e215995ba07ee46b61645b78bff010a14e5a92"),
            modelFile("decoder_kv.ort", 32_403_688, "6e3828f1db4b634bc525cb8ba1f0b628ec56059168f0336ad060891c7c1c9154"),
            modelFile("encoder.ort", 7_569_200, "96dde726be90c4429f3bc458d04e3ea5bd1818a5fdcd0152edf4c07b8e405c07"),
            modelFile("frontend.ort", 8_324_600, "bbdf5edb120cb3df1adf9ebc07c35136539b007a7047fd148c6f2960fc56fcf1"),
            modelFile("streaming_config.json", 509, "74fe5ddebd63b17caf59e8a3b18c17547ff7bce1642050edbb1c3962674f8950"),
            modelFile("tokenizer.bin", 249_974, "6884b35fd6377d4c4d32336a0bc152f36b64d1e45b6503683cdc238250a8472d"),
        ),
    ),

    ENGLISH_SMALL(
        "en",
        "English (Small)",
        "small-streaming-en",
        4,
        "https://download.moonshine.ai/model/small-streaming-en/quantized",
        listOf(
            modelFile("adapter.ort", 2_867_424, "d8493e0ac76a198b309a8be6f74b3101e235f773ffe5d6b378278cd7e4177992"),
            modelFile("cross_kv.ort", 5_298_736, "6e57d1361717e00d73336a0c3beafedae784b1e537905ad253dee33db4007466"),
            modelFile("decoder_kv.ort", 81_435_904, "d5adfcfaa6e582144791f1568bd0f683852c7bfbb8c79acad97499da05e4ffcf"),
            modelFile("encoder.ort", 43_853_224, "3b21d02eff6aa5651524ada4271d37c1d7bba4eb3d256415074f2cfdbaeb526a"),
            modelFile("frontend.ort", 30_984_200, "e086451043c1c8652a9614e4a4a81d5807221b611584a3cf31f73779d5900003"),
            modelFile("streaming_config.json", 512, "26f02b6afb22d60871a5efd85c3d38e569cc0ddb6c5eb6e93d3260152ae8a47a"),
            modelFile("tokenizer.bin", 249_974, "6884b35fd6377d4c4d32336a0bc152f36b64d1e45b6503683cdc238250a8472d"),
        ),
    ),

    ENGLISH_MEDIUM(
        "en",
        "English (Medium)",
        "medium-streaming-en",
        5,
        "https://download.moonshine.ai/model/medium-streaming-en/quantized",
        listOf(
            modelFile("adapter.ort", 3_647_712, "16307442b7f4229f2f1511fc51b545cec9616e55872c588f3a297bbc6f4762ea"),
            modelFile("cross_kv.ort", 11_544_952, "354b9a955caeb768b528f447f0a36ce4b850ca7b4531900165df304d97904fba"),
            modelFile("decoder_kv.ort", 146_216_448, "fa67aa87521247f5bf44d3e44d4e4978e58c1f114249c3c6909c882624056715"),
            modelFile("encoder.ort", 94_202_872, "a5f11167a62eef61787fe8410453257d6ddb8eba90af461a9604e5f2e93d5322"),
            modelFile("frontend.ort", 47_467_256, "378fe8a5d7090a1b9ab88bbb1fc95bde010cdd64ec23419350d2d23c675636e9"),
            modelFile("streaming_config.json", 513, "28e83b7a28e91472692a035e0dae3116422ae43aeb2bef5ed822c44ce89b88af"),
            modelFile("tokenizer.bin", 249_974, "6884b35fd6377d4c4d32336a0bc152f36b64d1e45b6503683cdc238250a8472d"),
        ),
    );

    val expectedSizeBytes: Long get() = modelFileContracts.sumOf(MoonshineModelFile::byteCount)

    companion object {
        fun forModelId(modelId: String): MoonshineLanguage = when (modelId) {
            "moonshine-small-streaming" -> ENGLISH_SMALL
            "moonshine-medium-streaming" -> ENGLISH_MEDIUM
            else -> ENGLISH_TINY
        }
    }
}
