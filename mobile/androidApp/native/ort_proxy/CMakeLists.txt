cmake_minimum_required(VERSION 3.22.1)
project(ort_proxy LANGUAGES C)

add_library(onnxruntime SHARED ort_proxy.c)

target_compile_options(onnxruntime PRIVATE
    -Oz
    -flto=thin
    -fvisibility=hidden
    -ffunction-sections
    -fdata-sections
)

target_link_options(onnxruntime PRIVATE
    -flto=thin
    -Wl,--gc-sections
    # Google Play 16 KB page-size requirement: every LOAD segment must be
    # 16 KB-aligned or the app cannot run on 16 KB-page devices.
    -Wl,-z,max-page-size=16384
)

target_link_libraries(onnxruntime
    dl
    log
)
