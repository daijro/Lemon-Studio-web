// Browser C++ runtime boundary; the DSP headers are unchanged.
#include <cstddef>
extern "C" void *lemon_cpp_alloc(std::size_t);
extern "C" void lemon_cpp_free(void *);
void *operator new(std::size_t n) { return lemon_cpp_alloc(n); }
void *operator new[](std::size_t n) { return lemon_cpp_alloc(n); }
void operator delete(void *p) noexcept { lemon_cpp_free(p); }
void operator delete[](void *p) noexcept { lemon_cpp_free(p); }
void operator delete(void *p, std::size_t) noexcept { lemon_cpp_free(p); }
void operator delete[](void *p, std::size_t) noexcept { lemon_cpp_free(p); }
namespace std { inline namespace __2 {
[[noreturn]] void __libcpp_verbose_abort(char const *, ...) noexcept { __builtin_trap(); }
}}
