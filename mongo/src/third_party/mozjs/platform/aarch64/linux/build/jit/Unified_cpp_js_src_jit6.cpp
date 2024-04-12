#define MOZ_UNIFIED_BUILD
#include "jit/JSONSpewer.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "jit/JSONSpewer.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "jit/JSONSpewer.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "jit/Jit.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "jit/Jit.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "jit/Jit.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "jit/JitContext.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "jit/JitContext.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "jit/JitContext.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "jit/JitFrames.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "jit/JitFrames.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "jit/JitFrames.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "jit/JitOptions.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "jit/JitOptions.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "jit/JitOptions.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif
#include "jit/JitScript.cpp"
#ifdef PL_ARENA_CONST_ALIGN_MASK
#error "jit/JitScript.cpp uses PL_ARENA_CONST_ALIGN_MASK, so it cannot be built in unified mode."
#undef PL_ARENA_CONST_ALIGN_MASK
#endif
#ifdef INITGUID
#error "jit/JitScript.cpp defines INITGUID, so it cannot be built in unified mode."
#undef INITGUID
#endif