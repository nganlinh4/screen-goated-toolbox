use crate::api::realtime_audio::model_loader::FileContract;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZipformerModelFile {
    pub name: &'static str,
    pub byte_count: u64,
    pub sha256: &'static str,
}

impl ZipformerModelFile {
    pub(super) fn contract(self) -> FileContract {
        FileContract {
            name: self.name,
            url: "",
            size_bytes: self.byte_count,
            sha256: self.sha256,
        }
    }
}

const EN_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder.onnx",
        byte_count: 70_092_599,
        sha256: "d4881c57449d581e0770fd53fa66c2fdc6cd167d92ece7c715e603defc96d9d4",
    },
    ZipformerModelFile {
        name: "decoder.onnx",
        byte_count: 617_488,
        sha256: "455ba38466fce8d5a57e7db68a323b684079ca4d9e1dd93a740d9b2429aae3b1",
    },
    ZipformerModelFile {
        name: "joiner.onnx",
        byte_count: 336_817,
        sha256: "d406f616736350e2a7df3e39398b78eb2fc1a2ca6973a19d3853fa3227e25b52",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 6_310,
        sha256: "396dbeb5f4858875690716084f54e90d339679d0ba3e6b5b584f3d7589254d2d",
    },
];
const KO_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder-epoch-99-avg-1.int8.onnx",
        byte_count: 126_968_852,
        sha256: "8d0b1aa24fbedd4e3948564ab7facd151b8ce9b0c48fc987c541de2de3af5697",
    },
    ZipformerModelFile {
        name: "decoder-epoch-99-avg-1.onnx",
        byte_count: 11_309_084,
        sha256: "b29cfb4575141e50a30a22b2c4579934f3d4f45b83c9c8c08c3aef5a3fa7abfc",
    },
    ZipformerModelFile {
        name: "joiner-epoch-99-avg-1.int8.onnx",
        byte_count: 2_581_421,
        sha256: "128b80a66a1f718488af8560f9d15895109b99ff3e573f0a0130e03774ef1ced",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 60_246,
        sha256: "016bdf0965029263b7ad01b742366ee542ef0bef38261510e8176ff6f2e9e668",
    },
    ZipformerModelFile {
        name: "bpe.model",
        byte_count: 314_212,
        sha256: "1491bc92c47dfda4225f5b8930fba3cfa34c3b1ccd25e7d96c630a262f3e918d",
    },
];
const ZH_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder.int8.onnx",
        byte_count: 161_141_793,
        sha256: "5ac51e27981bb4dab01bb9be4958453ba50c3b61c063ddda0eab23fd3671aa4f",
    },
    ZipformerModelFile {
        name: "decoder.onnx",
        byte_count: 5_165_083,
        sha256: "06522ad63cec0fdf6809f4e1db9bb4f7d710c34582e3b35db62ac60eccafac7e",
    },
    ZipformerModelFile {
        name: "joiner.int8.onnx",
        byte_count: 1_033_416,
        sha256: "b34584dc6f561089e1d747fedebb3765f2caa72c927ef54d7ca55e5ae40a814b",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 20_628,
        sha256: "6193c7ea1c96d0d9a1e9652789b40d13a8a913b434a5451e93158f5a09fd6652",
    },
];
const FR_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder.onnx",
        byte_count: 70_092_599,
        sha256: "e02facae1daf6f1f13da67ea3ace7c722516d0868d1768d78c0580bc22cc0c5b",
    },
    ZipformerModelFile {
        name: "decoder.onnx",
        byte_count: 617_488,
        sha256: "6aed547570e3ab5afc05429a017cedd3a056c16df3baa5703f02461cefa25bac",
    },
    ZipformerModelFile {
        name: "joiner.onnx",
        byte_count: 336_817,
        sha256: "a51eec759bcdcaae2614686fa2a8b57417b2d420dd55a5a5558b388d35a9b2b6",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 5_415,
        sha256: "fedfb9c844bfb2bf14171f8184863e3d617b815a8667bdd9fc9a3149fde73298",
    },
];
const DE_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder.onnx",
        byte_count: 70_091_557,
        sha256: "6e83993d6967ec7a3498b055b7e85ace85b5d64d1b1e8773cb29a43a11f5edb5",
    },
    ZipformerModelFile {
        name: "decoder.onnx",
        byte_count: 617_489,
        sha256: "94a29592b403c53fa2231b478637da1ab4abcef7f5e46e432098416a4a3ed562",
    },
    ZipformerModelFile {
        name: "joiner.onnx",
        byte_count: 336_817,
        sha256: "28356bff070aea51ab1d725a3278e81d19f9300f860d3248a7014292264df15a",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 5_606,
        sha256: "86e8370994ff2c01149ba8c4f8709aa93cdc18914b27a717e291e96faf39a6eb",
    },
];
const ES_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder.onnx",
        byte_count: 154_878_102,
        sha256: "2d9f5ef87d1a5257f8a6687e21501c56f3aa2fcbfcfab9364dcc4ce4e06ae81b",
    },
    ZipformerModelFile {
        name: "decoder.onnx",
        byte_count: 617_488,
        sha256: "d4ce176b94b25f7acc88717bc3f704fcf5d6e131aaac2e0cabab3885541181ee",
    },
    ZipformerModelFile {
        name: "joiner.onnx",
        byte_count: 336_817,
        sha256: "dae35df88d676e320fcdb99217328e66dcf722bf11b0f2459e14ddb5b982ded5",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 6_385,
        sha256: "1be5e0a58e05d06d327df4c6b7b5e4f8aba01da6981eb016fcaceafc6a56680f",
    },
];
const RU_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder.onnx",
        byte_count: 90_994_145,
        sha256: "e9c27453e618bc97cf8a10169f34c104bd478166522907fcd122a46a88c78c69",
    },
    ZipformerModelFile {
        name: "decoder.onnx",
        byte_count: 2_093_080,
        sha256: "89b3088a9e20e1ef7f2e85ce1a3478afe6a9c4ac57369cabcc4beb8e95328ea0",
    },
    ZipformerModelFile {
        name: "joiner.onnx",
        byte_count: 1_026_462,
        sha256: "dde0c7f3be0a16113a3e042c79a492c48667c07a8c1e9422ffe81c768aad4838",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 6_388,
        sha256: "93bbbc0bae6b78c0bbb743d4aa9fded3bb5ff3aac5f0200e3a769a5a05e0fdf6",
    },
    ZipformerModelFile {
        name: "bpe.model",
        byte_count: 246_184,
        sha256: "c7a756aeb3550417d6b2ed3efde9a7aa3eea54787d4eac011e9cce6090c9c64a",
    },
];
const ALL8_FILES: &[ZipformerModelFile] = &[
    ZipformerModelFile {
        name: "encoder-epoch-75-avg-11-chunk-16-left-128.int8.onnx",
        byte_count: 296_583_597,
        sha256: "f9001ed7a9e46d0294438c1a30cd7c72d1cc4bdd4e7880edbcda36f67081e32e",
    },
    ZipformerModelFile {
        name: "decoder-epoch-75-avg-11-chunk-16-left-128.onnx",
        byte_count: 33_837_085,
        sha256: "7ebc63f34b21c8efb4a41a5a2eee7fe1448829ce0230ecc5369e67fc14d90d48",
    },
    ZipformerModelFile {
        name: "joiner-epoch-75-avg-11-chunk-16-left-128.int8.onnx",
        byte_count: 8_257_421,
        sha256: "db88e3172323551abaa99b91b18fb422a27ea4a834fd0db10389f9478816f917",
    },
    ZipformerModelFile {
        name: "tokens.txt",
        byte_count: 195_244,
        sha256: "784f24950f6bcce1b0021035632dd60fd4617ecd8ca0581ab57d7b39d77ba5ab",
    },
    ZipformerModelFile {
        name: "bpe.model",
        byte_count: 476_049,
        sha256: "027731f33cff7266f2878c6fb7e478cf4af983962e311bb565112792794c13cd",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZipformerLanguage {
    English,
    Korean,
    Chinese,
    French,
    German,
    Spanish,
    Russian,
    All8Lang,
}

impl ZipformerLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Korean => "ko",
            Self::Chinese => "zh",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
            Self::Russian => "ru",
            Self::All8Lang => "all-8",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Korean => "Korean",
            Self::Chinese => "Chinese",
            Self::French => "French",
            Self::German => "German",
            Self::Spanish => "Spanish",
            Self::Russian => "Russian",
            Self::All8Lang => "AR,EN,ID,JA,RU,TH,VI,ZH",
        }
    }

    pub fn model_dir_name(self) -> &'static str {
        match self {
            Self::English => "streaming-zipformer-en-kroko",
            Self::Korean => "streaming-zipformer-korean",
            Self::Chinese => "streaming-zipformer-zh",
            Self::French => "streaming-zipformer-fr-kroko",
            Self::German => "streaming-zipformer-de-kroko",
            Self::Spanish => "streaming-zipformer-es-kroko",
            Self::Russian => "streaming-zipformer-small-ru-vosk",
            Self::All8Lang => "streaming-zipformer-multilingual-8lang",
        }
    }

    pub fn download_base_url(self) -> &'static str {
        match self {
            Self::English => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/572aaf4e2e0c603c3fc2a574d096e755a178faa1"
            }
            Self::Korean => {
                "https://modelscope.cn/models/k2-fsa/sherpa-onnx-streaming-zipformer-korean-2024-06-16/resolve/e5a1c5c5e52de4577aeddd4689f8b3844af36c7d"
            }
            Self::Chinese => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30/resolve/ad658fa0201659a09ea3c176129a191c77ecae8f"
            }
            Self::French => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-fr-kroko-2025-08-06/resolve/08b84b7b7cf519be9817e9c16919d96a7a8bad91"
            }
            Self::German => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-de-kroko-2025-08-06/resolve/887db3d083240198c2d2b99fb66cfcfe6948ced8"
            }
            Self::Spanish => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-es-kroko-2025-08-06/resolve/20cf7a4921613397841d31168796cade5b866585"
            }
            Self::Russian => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-small-ru-vosk-2025-08-16/resolve/b26590f0f87c179d5ea76ed08aa017ad5a8ae8b3"
            }
            Self::All8Lang => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10/resolve/c6726c1147387ad2a11148b33973135d92a55e6c"
            }
        }
    }

    pub fn model_files(self) -> &'static [ZipformerModelFile] {
        match self {
            Self::English => EN_FILES,
            Self::Korean => KO_FILES,
            Self::Chinese => ZH_FILES,
            Self::French => FR_FILES,
            Self::German => DE_FILES,
            Self::Spanish => ES_FILES,
            Self::Russian => RU_FILES,
            Self::All8Lang => ALL8_FILES,
        }
    }

    pub fn has_native_punctuation(self) -> bool {
        matches!(
            self,
            Self::English | Self::Korean | Self::French | Self::German | Self::Spanish
        )
    }

    pub fn sherpa_model_type(self) -> &'static str {
        match self {
            Self::English | Self::French | Self::German | Self::Spanish => "zipformer2",
            _ => "",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "en" | "all" => Self::English,
            "ko" => Self::Korean,
            "zh" => Self::Chinese,
            "fr" => Self::French,
            "de" => Self::German,
            "es" => Self::Spanish,
            "ru" => Self::Russian,
            "all-8" => Self::All8Lang,
            _ => Self::English,
        }
    }

    pub(super) fn encoder_file(self) -> &'static str {
        self.model_files()
            .iter()
            .find(|file| file.name.contains("encoder"))
            .unwrap()
            .name
    }

    pub(super) fn decoder_file(self) -> &'static str {
        self.model_files()
            .iter()
            .find(|file| file.name.contains("decoder"))
            .unwrap()
            .name
    }

    pub(super) fn joiner_file(self) -> &'static str {
        self.model_files()
            .iter()
            .find(|file| file.name.contains("joiner"))
            .unwrap()
            .name
    }
}

#[cfg(test)]
mod tests {
    use super::ZipformerLanguage;
    use serde::Deserialize;

    const ALL: [ZipformerLanguage; 8] = [
        ZipformerLanguage::English,
        ZipformerLanguage::Korean,
        ZipformerLanguage::Chinese,
        ZipformerLanguage::French,
        ZipformerLanguage::German,
        ZipformerLanguage::Spanish,
        ZipformerLanguage::Russian,
        ZipformerLanguage::All8Lang,
    ];

    #[derive(Deserialize)]
    struct Catalog {
        languages: Vec<CatalogEntry>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CatalogEntry {
        code: String,
        model_name: String,
        download_base_url: String,
        has_native_punctuation: bool,
        sherpa_model_type: String,
        model_files: Vec<CatalogFile>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CatalogFile {
        name: String,
        byte_count: u64,
        sha256: String,
    }

    #[test]
    fn windows_zipformer_catalog_matches_parity_fixture() {
        let catalog: Catalog = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/zipformer-catalog/catalog.json"
        )))
        .expect("fixture json");
        assert_eq!(catalog.languages.len(), ALL.len());
        for lang in ALL {
            let entry = catalog
                .languages
                .iter()
                .find(|entry| entry.code == lang.code())
                .unwrap_or_else(|| panic!("no fixture entry for code {}", lang.code()));
            assert_eq!(lang.model_dir_name(), entry.model_name, "{}", lang.code());
            assert_eq!(
                lang.download_base_url(),
                entry.download_base_url,
                "{}",
                lang.code()
            );
            assert!(!entry.download_base_url.ends_with("/main"));
            assert!(!entry.download_base_url.ends_with("/master"));
            assert_eq!(
                lang.has_native_punctuation(),
                entry.has_native_punctuation,
                "{}",
                lang.code()
            );
            assert_eq!(
                lang.sherpa_model_type(),
                entry.sherpa_model_type,
                "{}",
                lang.code()
            );
            let actual = lang.model_files();
            assert_eq!(actual.len(), entry.model_files.len(), "{}", lang.code());
            for (file, expected) in actual.iter().zip(&entry.model_files) {
                assert_eq!(file.name, expected.name, "{}", lang.code());
                assert_eq!(file.byte_count, expected.byte_count, "{}", lang.code());
                assert_eq!(file.sha256, expected.sha256, "{}", lang.code());
                assert_eq!(file.sha256.len(), 64);
                assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
            }
        }
    }
}
