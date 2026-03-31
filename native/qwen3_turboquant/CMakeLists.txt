cmake_minimum_required(VERSION 3.26)
project(sgt_qwen3_turboquant LANGUAGES CXX CUDA)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CUDA_STANDARD 17)
set(CMAKE_WINDOWS_EXPORT_ALL_SYMBOLS ON)

add_library(sgt_qwen3_turboquant SHARED
    src/sgt_qwen3_runtime_stub.cu
)

target_include_directories(sgt_qwen3_turboquant
    PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/include
)

set_target_properties(sgt_qwen3_turboquant PROPERTIES
    OUTPUT_NAME "sgt_qwen3_turboquant"
    CUDA_SEPARABLE_COMPILATION ON
)
