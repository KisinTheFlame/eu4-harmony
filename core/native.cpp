// SEH boundary between the Rust core and game code.
//
// eu4h_native_call invokes a game (or extension) function with integer/pointer
// arguments and turns a structured exception into an error code instead of a
// crash. eu4h_observer_thunk returns per-slot detours that run Rust "before"
// and "after" callbacks around the original function without leaving a Rust
// frame on the stack while the original runs, so the game's own SEH and C++
// exceptions unwind normally.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <intrin.h>
#include <array>
#include <cstddef>
#include <cstdint>
#include <utility>

using Word = uintptr_t;
struct HookFrame {
    Word args[12];
    Word result;
    Word return_address;
    Word slot;
    Word fault;
};
using Callback = void (*)(HookFrame*);
struct Slot {
    Word original;
    Callback before;
    Callback after;
};
constexpr size_t kSlots = 48;
static Slot slots[kSlots]{};
template <size_t> using Arg = Word;

template <size_t S, size_t... I> struct Observer {
    __declspec(noinline) static Word call(Arg<I>... args) {
        HookFrame frame{{args...}, 0, reinterpret_cast<Word>(_ReturnAddress()), S, 0};
        if (slots[S].before) slots[S].before(&frame);
        using Function = Word (*)(Arg<I>...);
        __try {
            frame.result = reinterpret_cast<Function>(slots[S].original)(frame.args[I]...);
        } __finally {
            if (AbnormalTermination()) {
                frame.fault = 1;
                frame.result = ~Word(0);
            }
            if (slots[S].after) slots[S].after(&frame);
        }
        return frame.result;
    }
};
template <size_t S, size_t... I> Word observer(std::index_sequence<I...>) {
    return reinterpret_cast<Word>(&Observer<S, I...>::call);
}
template <size_t S> Word select_arity(unsigned argc) {
#define CASE(N) case N: return observer<S>(std::make_index_sequence<N>{})
    switch (argc) { CASE(0); CASE(1); CASE(2); CASE(3); CASE(4); CASE(5); CASE(6); CASE(7); CASE(8); CASE(9); CASE(10); CASE(11); CASE(12); default: return 0; }
#undef CASE
}
template <size_t... S> auto selectors(std::index_sequence<S...>) {
    return std::array<Word (*)(unsigned), sizeof...(S)>{&select_arity<S>...};
}
extern "C" Word eu4h_observer_thunk(unsigned slot, unsigned argc) {
    static const auto table = selectors(std::make_index_sequence<kSlots>{});
    return slot < kSlots ? table[slot](argc) : 0;
}
extern "C" void eu4h_observer_setup(unsigned slot, Word original, Callback before, Callback after) {
    if (slot < kSlots) slots[slot] = {original, before, after};
}
// How many functions can be observed, for the Rust side to count on.
extern "C" size_t eu4h_observer_slots() {
    return kSlots;
}
// The HookFrame's size and where its fields after args lie, for the Rust side
// to check its own against.
extern "C" void eu4h_hook_frame_layout(size_t out[5]) {
    out[0] = sizeof(HookFrame);
    out[1] = offsetof(HookFrame, result);
    out[2] = offsetof(HookFrame, return_address);
    out[3] = offsetof(HookFrame, slot);
    out[4] = offsetof(HookFrame, fault);
}

template <size_t... I> Word invoke(Word target, const Word* args, std::index_sequence<I...>) {
    using Function = Word (*)(Arg<I>...);
    return reinterpret_cast<Function>(target)(args[I]...);
}
extern "C" unsigned eu4h_native_call(Word target, const Word* args, unsigned count, Word* result) {
    if (!target || !result || count > 12) return ERROR_INVALID_PARAMETER;
    __try {
#define CASE(N) case N: *result = invoke(target, args, std::make_index_sequence<N>{}); break
        switch (count) { CASE(0); CASE(1); CASE(2); CASE(3); CASE(4); CASE(5); CASE(6); CASE(7); CASE(8); CASE(9); CASE(10); CASE(11); CASE(12); }
#undef CASE
        return 0;
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return GetExceptionCode();
    }
}
extern "C" __declspec(noinline) Word eu4h_test_access_violation() {
    return *reinterpret_cast<volatile Word*>(1);
}
