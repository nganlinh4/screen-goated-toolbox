cmake_minimum_required(VERSION 3.22.1)
project(ort_proxy)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_library(onnxruntime SHARED ort_proxy.cpp)

target_compile_options(onnxruntime PRIVATE
    -fvisibility=hidden
    -ffunction-sections
    -fdata-sections
)

target_link_options(onnxruntime PRIVATE
    -Wl,--gc-sections
    # Google Play 16 KB page-size requirement: every LOAD segment must be
    # 16 KB-aligned or the app cannot run on 16 KB-page devices.
    -Wl,-z,max-page-size=16384
)

target_link_libraries(onnxruntime
    dl
    log
)
