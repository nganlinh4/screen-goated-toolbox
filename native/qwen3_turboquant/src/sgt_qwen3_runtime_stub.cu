#include "sgt_qwen3_runtime.h"

#include <cuda_runtime.h>
#include <new>

namespace {
struct StubSession {
    int reserved;
};
}

extern "C" SGT_QWEN3_EXPORT int32_t sgt_qwen3_runtime_version(void) {
    return 1;
}

extern "C" SGT_QWEN3_EXPORT int32_t sgt_qwen3_probe_cuda(void) {
    int device_count = 0;
    const cudaError_t err = cudaGetDeviceCount(&device_count);
    if (err != cudaSuccess) {
        return 0;
    }
    return device_count > 0 ? 1 : 0;
}

extern "C" SGT_QWEN3_EXPORT void* sgt_qwen3_create_session(void) {
    return new (std::nothrow) StubSession{0};
}

extern "C" SGT_QWEN3_EXPORT void sgt_qwen3_destroy_session(void* session) {
    delete static_cast<StubSession*>(session);
}
