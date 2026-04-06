/**
 * Thin wrapper DLL for Moonshine Voice SDK.
 *
 * The Moonshine SDK ships static .lib files compiled with /MD (dynamic CRT).
 * Our main Rust binary uses /MT (static CRT), causing linker errors.
 * This DLL bridges the gap: compiled with /MD to match the SDK libs,
 * then loaded dynamically at runtime via libloading.
 *
 * Build: scripts/build_moonshine_wrapper.ps1
 */

#include <windows.h>

/* Pull in the Moonshine C API — all implementations come from the static libs */
#include "moonshine-c-api.h"

BOOL APIENTRY DllMain(HMODULE hModule, DWORD reason, LPVOID lpReserved) {
    (void)hModule;
    (void)lpReserved;
    switch (reason) {
    case DLL_PROCESS_ATTACH:
    case DLL_THREAD_ATTACH:
    case DLL_THREAD_DETACH:
    case DLL_PROCESS_DETACH:
        break;
    }
    return TRUE;
}
