package dev.screengoated.toolbox.mobile.service.moonshine

data class ZipformerModelFile(
    override val name: String,
    override val byteCount: Long,
    override val sha256: String,
) : ManagedModelFile

private fun modelFile(name: String, byteCount: Long, sha256: String) =
    ZipformerModelFile(name, byteCount, sha256)

/** Streaming Zipformer models shared with the Windows catalog. */
enum class ZipformerLanguage(
    val code: String,
    val displayName: String,
    val modelName: String,
    val downloadBaseUrl: String,
    val modelFileContracts: List<ZipformerModelFile>,
    /** sherpa-onnx model type hint. Empty lets the runtime inspect ONNX metadata. */
    val sherpaModelType: String = "",
) {
    ENGLISH(
        "en",
        "English",
        "streaming-zipformer-en-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/572aaf4e2e0c603c3fc2a574d096e755a178faa1",
        listOf(
            modelFile("encoder.onnx", 70_092_599, "d4881c57449d581e0770fd53fa66c2fdc6cd167d92ece7c715e603defc96d9d4"),
            modelFile("decoder.onnx", 617_488, "455ba38466fce8d5a57e7db68a323b684079ca4d9e1dd93a740d9b2429aae3b1"),
            modelFile("joiner.onnx", 336_817, "d406f616736350e2a7df3e39398b78eb2fc1a2ca6973a19d3853fa3227e25b52"),
            modelFile("tokens.txt", 6_310, "396dbeb5f4858875690716084f54e90d339679d0ba3e6b5b584f3d7589254d2d"),
        ),
    ),
    KOREAN(
        "ko",
        "Korean",
        "streaming-zipformer-korean",
        "https://modelscope.cn/models/k2-fsa/sherpa-onnx-streaming-zipformer-korean-2024-06-16/resolve/e5a1c5c5e52de4577aeddd4689f8b3844af36c7d",
        listOf(
            modelFile("encoder-epoch-99-avg-1.int8.onnx", 126_968_852, "8d0b1aa24fbedd4e3948564ab7facd151b8ce9b0c48fc987c541de2de3af5697"),
            modelFile("decoder-epoch-99-avg-1.onnx", 11_309_084, "b29cfb4575141e50a30a22b2c4579934f3d4f45b83c9c8c08c3aef5a3fa7abfc"),
            modelFile("joiner-epoch-99-avg-1.int8.onnx", 2_581_421, "128b80a66a1f718488af8560f9d15895109b99ff3e573f0a0130e03774ef1ced"),
            modelFile("tokens.txt", 60_246, "016bdf0965029263b7ad01b742366ee542ef0bef38261510e8176ff6f2e9e668"),
            modelFile("bpe.model", 314_212, "1491bc92c47dfda4225f5b8930fba3cfa34c3b1ccd25e7d96c630a262f3e918d"),
        ),
    ),
    CHINESE(
        "zh",
        "Chinese",
        "streaming-zipformer-zh",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30/resolve/ad658fa0201659a09ea3c176129a191c77ecae8f",
        listOf(
            modelFile("encoder.int8.onnx", 161_141_793, "5ac51e27981bb4dab01bb9be4958453ba50c3b61c063ddda0eab23fd3671aa4f"),
            modelFile("decoder.onnx", 5_165_083, "06522ad63cec0fdf6809f4e1db9bb4f7d710c34582e3b35db62ac60eccafac7e"),
            modelFile("joiner.int8.onnx", 1_033_416, "b34584dc6f561089e1d747fedebb3765f2caa72c927ef54d7ca55e5ae40a814b"),
            modelFile("tokens.txt", 20_628, "6193c7ea1c96d0d9a1e9652789b40d13a8a913b434a5451e93158f5a09fd6652"),
        ),
    ),
    FRENCH(
        "fr",
        "French",
        "streaming-zipformer-fr-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-fr-kroko-2025-08-06/resolve/08b84b7b7cf519be9817e9c16919d96a7a8bad91",
        listOf(
            modelFile("encoder.onnx", 70_092_599, "e02facae1daf6f1f13da67ea3ace7c722516d0868d1768d78c0580bc22cc0c5b"),
            modelFile("decoder.onnx", 617_488, "6aed547570e3ab5afc05429a017cedd3a056c16df3baa5703f02461cefa25bac"),
            modelFile("joiner.onnx", 336_817, "a51eec759bcdcaae2614686fa2a8b57417b2d420dd55a5a5558b388d35a9b2b6"),
            modelFile("tokens.txt", 5_415, "fedfb9c844bfb2bf14171f8184863e3d617b815a8667bdd9fc9a3149fde73298"),
        ),
    ),
    GERMAN(
        "de",
        "German",
        "streaming-zipformer-de-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-de-kroko-2025-08-06/resolve/887db3d083240198c2d2b99fb66cfcfe6948ced8",
        listOf(
            modelFile("encoder.onnx", 70_091_557, "6e83993d6967ec7a3498b055b7e85ace85b5d64d1b1e8773cb29a43a11f5edb5"),
            modelFile("decoder.onnx", 617_489, "94a29592b403c53fa2231b478637da1ab4abcef7f5e46e432098416a4a3ed562"),
            modelFile("joiner.onnx", 336_817, "28356bff070aea51ab1d725a3278e81d19f9300f860d3248a7014292264df15a"),
            modelFile("tokens.txt", 5_606, "86e8370994ff2c01149ba8c4f8709aa93cdc18914b27a717e291e96faf39a6eb"),
        ),
    ),
    SPANISH(
        "es",
        "Spanish",
        "streaming-zipformer-es-kroko",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-es-kroko-2025-08-06/resolve/20cf7a4921613397841d31168796cade5b866585",
        listOf(
            modelFile("encoder.onnx", 154_878_102, "2d9f5ef87d1a5257f8a6687e21501c56f3aa2fcbfcfab9364dcc4ce4e06ae81b"),
            modelFile("decoder.onnx", 617_488, "d4ce176b94b25f7acc88717bc3f704fcf5d6e131aaac2e0cabab3885541181ee"),
            modelFile("joiner.onnx", 336_817, "dae35df88d676e320fcdb99217328e66dcf722bf11b0f2459e14ddb5b982ded5"),
            modelFile("tokens.txt", 6_385, "1be5e0a58e05d06d327df4c6b7b5e4f8aba01da6981eb016fcaceafc6a56680f"),
        ),
    ),
    RUSSIAN(
        "ru",
        "Russian",
        "streaming-zipformer-small-ru-vosk",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-small-ru-vosk-2025-08-16/resolve/b26590f0f87c179d5ea76ed08aa017ad5a8ae8b3",
        listOf(
            modelFile("encoder.onnx", 90_994_145, "e9c27453e618bc97cf8a10169f34c104bd478166522907fcd122a46a88c78c69"),
            modelFile("decoder.onnx", 2_093_080, "89b3088a9e20e1ef7f2e85ce1a3478afe6a9c4ac57369cabcc4beb8e95328ea0"),
            modelFile("joiner.onnx", 1_026_462, "dde0c7f3be0a16113a3e042c79a492c48667c07a8c1e9422ffe81c768aad4838"),
            modelFile("tokens.txt", 6_388, "93bbbc0bae6b78c0bbb743d4aa9fded3bb5ff3aac5f0200e3a769a5a05e0fdf6"),
            modelFile("bpe.model", 246_184, "c7a756aeb3550417d6b2ed3efde9a7aa3eea54787d4eac011e9cce6090c9c64a"),
        ),
    ),
    ALL_8LANG(
        "all-8",
        "AR, EN, ID, JA, RU, TH, VI, ZH",
        "streaming-zipformer-multilingual-8lang",
        "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10/resolve/c6726c1147387ad2a11148b33973135d92a55e6c",
        listOf(
            modelFile("encoder-epoch-75-avg-11-chunk-16-left-128.int8.onnx", 296_583_597, "f9001ed7a9e46d0294438c1a30cd7c72d1cc4bdd4e7880edbcda36f67081e32e"),
            modelFile("decoder-epoch-75-avg-11-chunk-16-left-128.onnx", 33_837_085, "7ebc63f34b21c8efb4a41a5a2eee7fe1448829ce0230ecc5369e67fc14d90d48"),
            modelFile("joiner-epoch-75-avg-11-chunk-16-left-128.int8.onnx", 8_257_421, "db88e3172323551abaa99b91b18fb422a27ea4a834fd0db10389f9478816f917"),
            modelFile("tokens.txt", 195_244, "784f24950f6bcce1b0021035632dd60fd4617ecd8ca0581ab57d7b39d77ba5ab"),
            modelFile("bpe.model", 476_049, "027731f33cff7266f2878c6fb7e478cf4af983962e311bb565112792794c13cd"),
        ),
    );

    val modelFiles: List<String> get() = modelFileContracts.map(ZipformerModelFile::name)

    val hasNativePunctuation: Boolean get() = when (this) {
        ENGLISH, KOREAN, FRENCH, GERMAN, SPANISH -> true
        CHINESE, RUSSIAN, ALL_8LANG -> false
    }

    fun sherpaEncoder(): String = modelFiles.first { it.contains("encoder") }
    fun sherpaDecoder(): String = modelFiles.first { it.contains("decoder") }
    fun sherpaJoiner(): String = modelFiles.first { it.contains("joiner") }
    val bpeVocabFile: String? get() = modelFiles.find { it == "bpe.model" }

    companion object {
        fun fromCode(code: String): ZipformerLanguage? = entries.find { it.code == code }
    }
}
