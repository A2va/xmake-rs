#undef NDEBUG
#include <assert.h>
#include <bar/bar.h>  

#ifndef TARGET_STATIC
    #ifdef _WIN32
        #define TARGET_DLL_EXPORT __declspec(dllexport)
        #define TARGET_DLL_IMPORT __declspec(dllimport)
    #else
        #define TARGET_DLL_EXPORT [[gnu::visibility("default")]]
        #define TARGET_DLL_IMPORT [[gnu::visibility("default")]]
    #endif
#else
    #define TARGET_DLL_EXPORT
    #define TARGET_DLL_IMPORT
#endif

#ifdef TARGET_BUILD
    #define TARGET_PUBLIC_API TARGET_DLL_EXPORT
#else
    #define TARGET_PUBLIC_API TARGET_DLL_IMPORT
#endif

TARGET_PUBLIC_API int target();

int target() {
    int b = bar();
    assert(b == 456);
    return 789;
}
