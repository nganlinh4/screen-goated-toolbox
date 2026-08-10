pub struct ToolRuntimeLocaleText {
    pub kokoro_downloading_message: &'static str,
    pub kokoro_downloading_title: &'static str,
    pub magpie_downloading_message: &'static str,
    pub magpie_downloading_title: &'static str,
    pub parakeet_downloading_file: &'static str,
    pub parakeet_downloading_message: &'static str,
    pub parakeet_tdt_downloading_title: &'static str,
    pub qwen3_1_7b_downloading_title: &'static str,
    pub qwen3_downloading_message: &'static str,
    pub qwen3_downloading_title: &'static str,
    pub qwen3_runtime_downloading_message: &'static str,
    pub qwen3_runtime_downloading_title: &'static str,
    pub step_audio_downloading_message: &'static str,
    pub step_audio_downloading_title: &'static str,
    pub vieneu_runtime_downloading_file_fmt: &'static str,
    pub vieneu_runtime_downloading_title: &'static str,
    pub vieneu_runtime_extracting: &'static str,
    pub vieneu_runtime_fetching_manifest: &'static str,
    pub vieneu_runtime_preparing_runtime: &'static str,
    pub vieneu_runtime_ready: &'static str,
}

pub fn get(language: &str) -> ToolRuntimeLocaleText {
    match language {
        "vi" => vietnamese(),
        "ko" => korean(),
        _ => english(),
    }
}

fn english() -> ToolRuntimeLocaleText {
    ToolRuntimeLocaleText {
        kokoro_downloading_message: "Please wait...",
        kokoro_downloading_title: "Downloading Kokoro 82M (0.3 GB)",
        magpie_downloading_message: "Please wait...",
        magpie_downloading_title: "Downloading Magpie-Multilingual + NanoCodec (~1.5 GB)",
        parakeet_downloading_file: "Downloading {}...",
        parakeet_downloading_message: "Please wait...",
        parakeet_tdt_downloading_title: "Downloading Parakeet TDT 0.6B v3",
        qwen3_1_7b_downloading_title: "Downloading Qwen3-ASR 1.7B",
        qwen3_downloading_message: "Please wait... this is a large local model.",
        qwen3_downloading_title: "Downloading Qwen3-ASR 0.6B",
        qwen3_runtime_downloading_message: "Please wait... runtime install downloads about 3 GB.",
        qwen3_runtime_downloading_title: "Downloading Qwen3-ASR CUDA Runtime",
        step_audio_downloading_message: "Please wait...",
        step_audio_downloading_title: "Downloading Step Audio EditX + Tokenizer (~4.2 GB)",
        vieneu_runtime_downloading_file_fmt: "Downloading {}...",
        vieneu_runtime_downloading_title: "Downloading VieNeu runtime",
        vieneu_runtime_extracting: "Extracting VieNeu runtime...",
        vieneu_runtime_fetching_manifest: "Fetching runtime manifest...",
        vieneu_runtime_preparing_runtime: "Preparing VieNeu runtime...",
        vieneu_runtime_ready: "Ready.",
    }
}

fn korean() -> ToolRuntimeLocaleText {
    ToolRuntimeLocaleText {
        kokoro_downloading_message: "잠시만 기다려주세요...",
        kokoro_downloading_title: "Kokoro 82M 모델 다운로드 중 (0.3 GB)",
        magpie_downloading_message: "잠시만 기다려주세요...",
        magpie_downloading_title: "Magpie-Multilingual + NanoCodec 다운로드 중 (~1.5 GB)",
        parakeet_downloading_file: "{} 다운로드 중...",
        parakeet_downloading_message: "잠시만 기다려주세요...",
        parakeet_tdt_downloading_title: "Parakeet TDT 0.6B v3 다운로드 중",
        qwen3_1_7b_downloading_title: "Qwen3-ASR 1.7B 다운로드 중",
        qwen3_downloading_message: "잠시만 기다려주세요... 큰 로컬 모델입니다.",
        qwen3_downloading_title: "Qwen3-ASR 0.6B 다운로드 중",
        qwen3_runtime_downloading_message: "잠시만 기다려주세요... 런타임 설치 시 약 3 GB를 다운로드합니다.",
        qwen3_runtime_downloading_title: "Qwen3-ASR CUDA 런타임 다운로드 중",
        step_audio_downloading_message: "잠시만 기다려주세요...",
        step_audio_downloading_title: "Step Audio EditX + Tokenizer 다운로드 중 (~4.2 GB)",
        vieneu_runtime_downloading_file_fmt: "{} 다운로드 중...",
        vieneu_runtime_downloading_title: "VieNeu 런타임 다운로드 중",
        vieneu_runtime_extracting: "VieNeu 런타임 압축 해제 중...",
        vieneu_runtime_fetching_manifest: "런타임 매니페스트 가져오는 중...",
        vieneu_runtime_preparing_runtime: "VieNeu 런타임 준비 중...",
        vieneu_runtime_ready: "준비 완료.",
    }
}

fn vietnamese() -> ToolRuntimeLocaleText {
    ToolRuntimeLocaleText {
        kokoro_downloading_message: "Vui lòng đợi...",
        kokoro_downloading_title: "Đang tải Kokoro 82M (0.3 GB)",
        magpie_downloading_message: "Vui lòng đợi...",
        magpie_downloading_title: "Đang tải Magpie-Multilingual + NanoCodec (~1,5 GB)",
        parakeet_downloading_file: "Đang tải {}...",
        parakeet_downloading_message: "Vui lòng đợi...",
        parakeet_tdt_downloading_title: "Đang tải Parakeet TDT 0.6B v3",
        qwen3_1_7b_downloading_title: "Đang tải Qwen3-ASR 1.7B",
        qwen3_downloading_message: "Vui lòng đợi... đây là model cục bộ khá lớn.",
        qwen3_downloading_title: "Đang tải Qwen3-ASR 0.6B",
        qwen3_runtime_downloading_message: "Vui lòng đợi... quá trình cài runtime tải khoảng 3 GB.",
        qwen3_runtime_downloading_title: "Đang tải Runtime CUDA Qwen3-ASR",
        step_audio_downloading_message: "Vui lòng đợi...",
        step_audio_downloading_title: "Đang tải Step Audio EditX + Tokenizer (~4,2 GB)",
        vieneu_runtime_downloading_file_fmt: "Đang tải {}...",
        vieneu_runtime_downloading_title: "Đang tải runtime VieNeu",
        vieneu_runtime_extracting: "Đang giải nén runtime VieNeu...",
        vieneu_runtime_fetching_manifest: "Đang lấy manifest runtime...",
        vieneu_runtime_preparing_runtime: "Đang chuẩn bị runtime VieNeu...",
        vieneu_runtime_ready: "Sẵn sàng.",
    }
}
