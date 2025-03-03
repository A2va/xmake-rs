#ifdef __cplusplus
extern "C" {
#endif

int add(int a, int b);

#ifdef __cplusplus
}
#endif

#include <vector>

int add(int a, int b) {
    std::vector<int> v;
    v.push_back(a);
    v.push_back(b);
    return v.at(0) + v.at(1);
}

