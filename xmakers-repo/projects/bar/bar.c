#undef NDEBUG
#include <assert.h>

#include <foo/foo.h>
#include "bar/bar.h"   

int bar() {
    int f = foo();
    assert(f == 123);
    return 456;    
}