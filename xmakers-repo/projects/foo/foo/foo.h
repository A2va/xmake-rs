#ifndef XMAKERS_FOO_H
#define XMAKERS_FOO_H

#ifndef FOO_STATIC
    #ifdef _WIN32
        #define FOO_DLL_EXPORT __declspec(dllexport)
        #define FOO_DLL_IMPORT __declspec(dllimport)
    #else
        #define FOO_DLL_EXPORT [[gnu::visibility("default")]]
        #define FOO_DLL_IMPORT [[gnu::visibility("default")]]
    #endif
#else
    #define FOO_DLL_EXPORT
    #define FOO_DLL_IMPORT
#endif

#ifdef FOO_BUILD
    #define FOO_PUBLIC_API FOO_DLL_EXPORT
#else
    #define FOO_PUBLIC_API FOO_DLL_IMPORT
#endif

FOO_PUBLIC_API int foo();
#endif