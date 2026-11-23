cmake_minimum_required(VERSION 3.22.1)
project(sgt_file_namespace LANGUAGES C)

add_library(sgt_file_namespace SHARED namespace.c namespace_jni.c)
target_compile_definitions(sgt_file_namespace PRIVATE _GNU_SOURCE)
target_compile_options(sgt_file_namespace PRIVATE
    -Wall -Wextra -Werror -Wconversion -Wshadow -Wstrict-prototypes
    -fvisibility=hidden -ffunction-sections -fdata-sections)
target_link_options(sgt_file_namespace PRIVATE
    -Wl,--gc-sections -Wl,-z,max-page-size=16384)
