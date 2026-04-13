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
)

target_link_libraries(onnxruntime
    dl
    log
)
