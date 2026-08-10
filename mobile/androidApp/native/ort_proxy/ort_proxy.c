#include <android/log.h>
#include <dlfcn.h>
#include <limits.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>

#define ORT_PROXY_TAG "OrtProxy"
#define ORT_REAL_LIBRARY "libonnxruntime_real.so"

typedef const void* (*ort_get_api_base_fn)(void);

static pthread_once_t init_once = PTHREAD_ONCE_INIT;
static void* real_handle;
static ort_get_api_base_fn real_get_api_base;

__attribute__((visibility("default"))) const void* OrtGetApiBase(void);

static void initialize_real_ort(void) {
  Dl_info info = {0};
  if (dladdr((const void*)&OrtGetApiBase, &info) == 0 || info.dli_fname == NULL) {
    __android_log_print(ANDROID_LOG_ERROR, ORT_PROXY_TAG, "dladdr failed");
    return;
  }

  const char* slash = strrchr(info.dli_fname, '/');
  if (slash == NULL) {
    __android_log_print(ANDROID_LOG_ERROR, ORT_PROXY_TAG, "proxy path has no directory");
    return;
  }

  char path[PATH_MAX];
  const size_t directory_length = (size_t)(slash - info.dli_fname);
  const int written = snprintf(
      path, sizeof(path), "%.*s/%s", (int)directory_length, info.dli_fname,
      ORT_REAL_LIBRARY);
  if (written < 0 || (size_t)written >= sizeof(path)) {
    __android_log_print(ANDROID_LOG_ERROR, ORT_PROXY_TAG, "real runtime path is too long");
    return;
  }

  real_handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
  if (real_handle == NULL) {
    __android_log_print(ANDROID_LOG_ERROR, ORT_PROXY_TAG, "dlopen failed: %s", dlerror());
    return;
  }

  real_get_api_base = (ort_get_api_base_fn)dlsym(real_handle, "OrtGetApiBase");
  if (real_get_api_base == NULL) {
    __android_log_print(ANDROID_LOG_ERROR, ORT_PROXY_TAG, "dlsym failed: %s", dlerror());
  }
}

__attribute__((visibility("default"))) const void* OrtGetApiBase(void) {
  (void)pthread_once(&init_once, initialize_real_ort);
  return real_get_api_base == NULL ? NULL : real_get_api_base();
}
