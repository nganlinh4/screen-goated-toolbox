#include <android/log.h>
#include <dlfcn.h>

#include <mutex>
#include <stdexcept>
#include <string>

namespace {

constexpr const char* kTag = "OrtProxy";
constexpr const char* kRealLibName = "libonnxruntime_real.so";

using OrtGetApiBaseFn = const void* (*)();

std::once_flag g_init_once;
void* g_real_handle = nullptr;
OrtGetApiBaseFn g_real_get_api_base = nullptr;
std::string g_init_error;

void LogI(const std::string& message) {
  __android_log_print(ANDROID_LOG_INFO, kTag, "%s", message.c_str());
}

void LogE(const std::string& message) {
  __android_log_print(ANDROID_LOG_ERROR, kTag, "%s", message.c_str());
}

std::string CurrentLibDir() {
  Dl_info info{};
  if (dladdr(reinterpret_cast<void*>(&CurrentLibDir), &info) == 0 || info.dli_fname == nullptr) {
    throw std::runtime_error("dladdr failed for ort proxy");
  }
  std::string path(info.dli_fname);
  const auto slash = path.find_last_of('/');
  if (slash == std::string::npos) {
    throw std::runtime_error("Could not resolve ort proxy directory");
  }
  return path.substr(0, slash);
}

void InitRealOrt() {
  try {
    const std::string real_path = CurrentLibDir() + "/" + kRealLibName;
    g_real_handle = dlopen(real_path.c_str(), RTLD_NOW | RTLD_LOCAL);
    if (g_real_handle == nullptr) {
      throw std::runtime_error(std::string("dlopen ") + kRealLibName + " failed: " + dlerror());
    }

    g_real_get_api_base = reinterpret_cast<OrtGetApiBaseFn>(dlsym(g_real_handle, "OrtGetApiBase"));
    if (g_real_get_api_base == nullptr) {
      throw std::runtime_error(std::string("dlsym OrtGetApiBase failed: ") + dlerror());
    }
    LogI(std::string("Forwarding OrtGetApiBase to ") + real_path);
  } catch (const std::exception& e) {
    g_init_error = e.what();
    LogE(g_init_error);
  }
}

}  // namespace

extern "C" __attribute__((visibility("default"))) const void* OrtGetApiBase() {
  std::call_once(g_init_once, InitRealOrt);
  if (g_real_get_api_base == nullptr) {
    return nullptr;
  }
  return g_real_get_api_base();
}
