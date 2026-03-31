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

SGT_QWEN3_EXPORT int32_t sgt_qwen3_runtime_version(void);
SGT_QWEN3_EXPORT int32_t sgt_qwen3_probe_cuda(void);
SGT_QWEN3_EXPORT void* sgt_qwen3_create_session(void);
SGT_QWEN3_EXPORT void sgt_qwen3_destroy_session(void* session);

#ifdef __cplusplus
}
#endif
