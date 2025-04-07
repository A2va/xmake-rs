int mode() {
#ifdef FOO_DEBUG
    return 1;
#elif FOO_RELEASE
    return 2;
#else
    return 0;
#endif
}
