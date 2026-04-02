#pragma once

#include <stdint.h>

#ifdef _WIN32
#define SGT_QWEN3_EXPORT __declspec(dllexport)
#else
#define SGT_QWEN3_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

SGT_QWEN3_EXPORT uint32_t sgt_qwen3_runtime_version(void);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_probe_cuda(const char** out_json, uintptr_t* out_len);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_create_runtime(
    const uint8_t* config_json,
    uintptr_t config_len,
    void** out_runtime
);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_destroy_runtime(void* runtime);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_create_session(
    void* runtime,
    const uint8_t* session_json,
    uintptr_t session_len,
    void** out_session
);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_destroy_session(void* session);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_append_pcm16(
    void* session,
    const int16_t* samples,
    uintptr_t sample_count,
    int32_t is_final
);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_step(
    void* session,
    const char** out_json,
    uintptr_t* out_len
);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_reset_session(void* session);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_last_error(
    void* handle_or_null,
    const char** out_json,
    uintptr_t* out_len
);

#ifdef __cplusplus
}
#endif
