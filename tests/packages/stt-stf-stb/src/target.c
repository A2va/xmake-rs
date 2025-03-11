#undef NDEBUG
#include <assert.h>
#include <bar/bar.h>   

int target() {
    int b = bar();
    assert(b == 456);
    return 789;
}
