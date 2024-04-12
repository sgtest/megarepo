#define MOZ_UNIFIED_BUILD
#include "wasm/WasmIonCompile.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmIonCompile.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmIonCompile.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmJS.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmJS.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmJS.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmLog.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmLog.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmLog.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmMemory.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmMemory.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmMemory.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmModule.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmModule.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmModule.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmModuleTypes.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmModuleTypes.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmModuleTypes.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif