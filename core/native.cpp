// SEH boundary between the Rust core and game code.
//
// eu4h_read copies game memory inside the process and turns a fault into an
// error code, so reading through a bad pointer fails instead of crashing.
// eu4h_native_call invokes a game (or extension) function with integer/pointer
// arguments and turns a structured exception into an error code instead of a
// crash. eu4h_observer_new makes a detour that runs Rust "before" and "after"
// callbacks around the original function without leaving a Rust frame on the
// stack while the original runs, so the game's own SEH and C++ exceptions
// unwind normally: a stub of the observer's own, written at run time, pushes
// its slot and jumps to the entry all observers share (observer.asm), which
// calls eu4h_observe.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <utility>

using Word = uintptr_t;
struct HookFrame {
    Word args[12];
    Word result;
    Word return_address;
    Word observers;
    Word fault;
};
using Callback = void (*)(HookFrame*);
// One observed function. `observers` is the Rust side's, handed back in every
// frame as it was given.
struct Slot {
    Word original;
    Callback before;
    Callback after;
    Word observers;
    unsigned argc;
};
template <size_t> using Arg = Word;

template <size_t... I> Word invoke(Word target, const Word* args, std::index_sequence<I...>) {
    using Function = Word (*)(Arg<I>...);
    return reinterpret_cast<Function>(target)(args[I]...);
}
// Calls `target` with the first `count` (at most 12) of `args`.
static Word call_with(Word target, const Word* args, unsigned count) {
#define CASE(N) case N: return invoke(target, args, std::make_index_sequence<N>{})
    switch (count) { CASE(0); CASE(1); CASE(2); CASE(3); CASE(4); CASE(5); CASE(6); CASE(7); CASE(8); CASE(9); CASE(10); CASE(11); CASE(12); default: return 0; }
#undef CASE
}

// What the shared entry calls: `stack` is where the observed function's
// caller left its return address, followed by the words its arguments are
// passed in, the four registers spilled into their home space and then the
// stack.
extern "C" Word eu4h_observe(const Slot* slot, const Word* stack) {
    HookFrame frame{{}, 0, stack[0], slot->observers, 0};
    for (unsigned i = 0; i < slot->argc; ++i) frame.args[i] = stack[1 + i];
    if (slot->before) slot->before(&frame);
    __try {
        frame.result = call_with(slot->original, frame.args, slot->argc);
    } __finally {
        if (AbnormalTermination()) {
            frame.fault = 1;
            frame.result = ~Word(0);
        }
        if (slot->after) slot->after(&frame);
    }
    return frame.result;
}

extern "C" void eu4h_observer_entry();
// A stub, which leaves every register as it was: push [rip + 0ah] (the slot);
// jmp [rip + 0ch] (the shared entry); int3 x 4; the slot; the entry.
constexpr size_t kStub = 32;
// Stubs are executable as soon as they are written, as MinHook wants of a
// detour. A page is made writable again only for the next stub, and only
// until eu4h_observer_seal: once a hook may reach its stubs, no more are
// written into it, so no stub in use ever changes.
constexpr size_t kStubPage = 64 * 1024;
static SRWLOCK stub_lock = SRWLOCK_INIT;
static unsigned char* stub_page;
static size_t stub_used = kStubPage;

static bool write_stub(unsigned char* stub, const Slot* slot) {
    DWORD old;
    if (!VirtualProtect(stub_page, kStubPage, PAGE_READWRITE, &old)) return false;
    const Word entry = reinterpret_cast<Word>(&eu4h_observer_entry);
    const unsigned char code[16] = {0xff, 0x35, 0x0a, 0, 0, 0, 0xff, 0x25, 0x0c, 0, 0, 0, 0xcc, 0xcc, 0xcc, 0xcc};
    std::memcpy(stub, code, 16);
    std::memcpy(stub + 16, &slot, 8);
    std::memcpy(stub + 24, &entry, 8);
    const bool executable = VirtualProtect(stub_page, kStubPage, PAGE_EXECUTE_READ, &old);
    FlushInstructionCache(GetCurrentProcess(), stub, kStub);
    return executable;
}

// A slot for a function taking `argc` integer/pointer arguments, and in
// `thunk` the detour that observes it; null when `argc` is over 12 or no
// memory is left. Slots and stubs are never freed: a thread may still be in
// one after its hook is removed.
extern "C" Slot* eu4h_observer_new(unsigned argc, Word observers, Word* thunk) {
    if (argc > 12) return nullptr;
    AcquireSRWLockExclusive(&stub_lock);
    if (stub_used + kStub > kStubPage) {
        if (auto page = static_cast<unsigned char*>(VirtualAlloc(nullptr, kStubPage, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READ))) {
            stub_page = page;
            stub_used = 0;
        }
    }
    Slot* slot = nullptr;
    if (stub_used + kStub <= kStubPage) {
        slot = new Slot{0, nullptr, nullptr, observers, argc};
        unsigned char* stub = stub_page + stub_used;
        stub_used += kStub;
        if (write_stub(stub, slot)) {
            *thunk = reinterpret_cast<Word>(stub);
        } else {
            delete slot;
            slot = nullptr;
        }
    }
    ReleaseSRWLockExclusive(&stub_lock);
    return slot;
}
extern "C" void eu4h_observer_setup(Slot* slot, Word original, Callback before, Callback after) {
    slot->original = original;
    slot->before = before;
    slot->after = after;
}
// The stubs written so far may run from now on: the next goes into a new page.
extern "C" void eu4h_observer_seal() {
    AcquireSRWLockExclusive(&stub_lock);
    stub_used = kStubPage;
    ReleaseSRWLockExclusive(&stub_lock);
}
// The HookFrame's size and where its fields after args lie, for the Rust side
// to check its own against.
extern "C" void eu4h_hook_frame_layout(size_t out[5]) {
    out[0] = sizeof(HookFrame);
    out[1] = offsetof(HookFrame, result);
    out[2] = offsetof(HookFrame, return_address);
    out[3] = offsetof(HookFrame, observers);
    out[4] = offsetof(HookFrame, fault);
}

// Where a fault happened: the instruction, and for an access violation the
// address it read or wrote and whether it wrote.
static int note_fault(const EXCEPTION_POINTERS* e, Word* fault) {
    const EXCEPTION_RECORD* r = e->ExceptionRecord;
    fault[0] = reinterpret_cast<Word>(r->ExceptionAddress);
    fault[1] = r->NumberParameters >= 2 ? r->ExceptionInformation[1] : 0;
    fault[2] = r->NumberParameters >= 1 ? r->ExceptionInformation[0] : 0;
    return EXCEPTION_EXECUTE_HANDLER;
}
extern "C" unsigned eu4h_native_call(Word target, const Word* args, unsigned count, Word* result, Word* fault) {
    if (!target || !result || !fault || count > 12) return ERROR_INVALID_PARAMETER;
    __try {
        *result = call_with(target, args, count);
        return 0;
    } __except (note_fault(GetExceptionInformation(), fault)) {
        return GetExceptionCode();
    }
}
// Only the faults a read can meet are handled; anything else is not ours.
static int read_fault(unsigned code) {
    return code == EXCEPTION_ACCESS_VIOLATION || code == EXCEPTION_IN_PAGE_ERROR || code == STATUS_GUARD_PAGE_VIOLATION ? EXCEPTION_EXECUTE_HANDLER
                                                                                                                        : EXCEPTION_CONTINUE_SEARCH;
}
extern "C" unsigned eu4h_read(const void* source, void* target, size_t count) {
    __try {
        std::memcpy(target, source, count);
        return 0;
    } __except (read_fault(GetExceptionCode())) {
        return GetExceptionCode();
    }
}
extern "C" __declspec(noinline) Word eu4h_test_access_violation() {
    return *reinterpret_cast<volatile Word*>(1);
}
