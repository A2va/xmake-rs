#ifndef XMAKERS_BAR_H
#define XMAKERS_BAR_H

#ifndef BAR_STATIC
    #ifdef _WIN32
        #define BAR_DLL_EXPORT __declspec(dllexport)
        #define BAR_DLL_IMPORT __declspec(dllimport)
    #else
        #define BAR_DLL_EXPORT [[gnu::visibility("default")]]
        #define BAR_DLL_IMPORT [[gnu::visibility("default")]]
    #endif
#else
    #define BAR_DLL_EXPORT
    #define BAR_DLL_IMPORT
#endif

#ifdef BAR_BUILD
    #define BAR_PUBLIC_API BAR_DLL_EXPORT
#else
    #define BAR_PUBLIC_API BAR_DLL_IMPORT
#endif

BAR_PUBLIC_API int bar();
#endif