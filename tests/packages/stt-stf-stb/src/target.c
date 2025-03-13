#undef NDEBUG
#include <assert.h>
// #include <bar/bar.h>  
#include <foo/foo.h> 

int target() {
    // int b = bar();
    // assert(b == 456);
    int f = foo();
    assert(f == 123);
    return 789;
}
