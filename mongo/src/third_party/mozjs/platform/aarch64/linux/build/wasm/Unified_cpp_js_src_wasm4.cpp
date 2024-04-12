#define MOZ_UNIFIED_BUILD
#include "wasm/WasmOpIter.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmOpIter.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmOpIter.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmProcess.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmProcess.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmProcess.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmRealm.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmRealm.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmRealm.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmSerialize.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmSerialize.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmSerialize.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmSignalHandlers.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmSignalHandlers.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmSignalHandlers.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "wasm/WasmStubs.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "wasm/WasmStubs.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "wasm/WasmStubs.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif