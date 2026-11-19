; The entry every observer's stub jumps to, the observer's slot pushed onto
; the stack above the return address (native.cpp). It spills the four argument
; registers into their home space, which the caller's stack arguments follow,
; and hands eu4h_observe the slot and that run of words, the return address
; before them. The game's code is compiled as a whole, and a caller may keep a
; value in a volatile register across a call of a function it knows leaves
; that register alone: the integer registers the original's caller could see
; are as they were when it returns, but for rax, its result. The unwind data
; counts the slot with the frame, so exceptions in the original pass through.
EXTERN eu4h_observe:PROC
EXTERN eu4h_resolve_stub:PROC
EXTERN eu4h_jump_stubs:QWORD

.data
ALIGN 16
stub_cache QWORD 1024 DUP(0)

.code
PUBLIC eu4h_clear_stub_cache
eu4h_clear_stub_cache PROC
    lea rcx, stub_cache
    xor edx, edx
clear_loop:
    mov qword ptr [rcx + rdx * 8], 0
    inc edx
    cmp edx, 1024
    jb clear_loop
    ret
eu4h_clear_stub_cache ENDP

; Slow-path helper for eu4h_call_rust: preserves target pointer in rax and
; argument registers (rdx, r8, r9, xmm0..xmm3; arg1 rcx is already saved in
; [eu4h_call_rust_rsp + 58h]) while resolving `ret_into_stub` in rcx and
; caching (rel32 << 32) | code32. Returns code32 in ecx (1-based index into
; eu4h_jump_stubs, or 0ffffffffh for a called stub).
eu4h_resolve_helper PROC FRAME
    sub rsp, 88h
    .allocstack 88h
    .endprolog
    mov [rsp + 20h], rax
    mov [rsp + 28h], rdx
    mov [rsp + 30h], r8
    mov [rsp + 38h], r9
    movdqa [rsp + 40h], xmm0
    movdqa [rsp + 50h], xmm1
    movdqa [rsp + 60h], xmm2
    movdqa [rsp + 70h], xmm3
    mov [rsp + 80h], rcx

    call eu4h_resolve_stub

    mov r10, [rsp + 80h]
    lea r11, stub_cache
    mov rdx, r10
    sub rdx, r11
    movsxd r8, edx
    cmp r8, rdx
    jne helper_skip_store
    mov r8d, edx
    shl r8, 32
    mov r9d, eax
    or r8, r9
    shr edx, 4
    and edx, 3ffh
    mov [r11 + rdx * 8], r8
helper_skip_store:
    mov ecx, eax
    mov rax, [rsp + 20h]
    mov rdx, [rsp + 28h]
    mov r8, [rsp + 30h]
    mov r9, [rsp + 38h]
    movdqa xmm0, [rsp + 40h]
    movdqa xmm1, [rsp + 50h]
    movdqa xmm2, [rsp + 60h]
    movdqa xmm3, [rsp + 70h]
    add rsp, 88h
    ret
eu4h_resolve_helper ENDP

; Trampoline called by `call_rust!`: rax is the Rust target, rbp holds the
; `call_rust!` frame pointer ([rbp] = game rbp, [rbp + 8] = entry rsp or
; caller return address), [rsp + 28h..48h] hold stack arguments 5..9.
; Records a machine frame so SEH and stack walkers unwind cleanly back to the
; jump site in the game (or the caller of a called stub).
PUBLIC eu4h_call_rust
eu4h_call_rust PROC FRAME
    sub rsp, 88h
    .pushframe
    .allocstack 60h
    .savereg rbp, 50h
    .endprolog

    mov [rsp + 58h], rcx
    mov r10, [rbp]
    mov [rsp + 50h], r10
    xor r10d, r10d
    mov [rsp + 68h], r10
    mov [rsp + 70h], r10
    mov [rsp + 80h], r10
    mov r10, [rbp + 8]
    mov [rsp + 60h], r10
    lea r11, [rbp + 10h]
    mov [rsp + 78h], r11

    mov r10, [rsp + 88h]
    lea r11, stub_cache
    sub r10, r11
    movsxd rcx, r10d
    cmp rcx, r10
    jne resolve_slow
    mov ecx, r10d
    shr r10d, 4
    and r10d, 3ffh
    mov r10, [r11 + r10 * 8]
    mov r11, r10
    shr r11, 32
    cmp r11d, ecx
    jne resolve_slow
    mov ecx, r10d
    test ecx, ecx
    jz resolve_slow

have_code:
    cmp ecx, 0ffffffffh
    je run_target
    mov ecx, ecx
    shl rcx, 4
    lea r10, eu4h_jump_stubs
    mov r10, [r10 + rcx]
    mov [rsp + 60h], r10
    lea r11, [rbp + 8]
    mov [rsp + 78h], r11

run_target:
    mov r10, [rsp + 0b0h]
    mov [rsp + 20h], r10
    mov r10, [rsp + 0b8h]
    mov [rsp + 28h], r10
    mov r10, [rsp + 0c0h]
    mov [rsp + 30h], r10
    mov r10, [rsp + 0c8h]
    mov [rsp + 38h], r10
    mov r10, [rsp + 0d0h]
    mov [rsp + 40h], r10
    mov rcx, [rsp + 58h]
    call rax
    ; Prevent Windows RtlVirtualUnwind and the sampler from treating the return
    ; site as inside the epilog (which would bypass UWOP_PUSH_MACHFRAME).
    nop
    add rsp, 88h
    ret

resolve_slow:
    mov rcx, [rsp + 88h]
    call eu4h_resolve_helper
    jmp have_code
eu4h_call_rust ENDP

eu4h_observer_entry PROC FRAME
    sub rsp, 30h
    .allocstack 38h
    .endprolog
    mov [rsp + 40h], rcx
    mov [rsp + 48h], rdx
    mov [rsp + 50h], r8
    mov [rsp + 58h], r9
    mov [rsp + 20h], r10
    mov [rsp + 28h], r11
    mov rcx, [rsp + 30h]
    lea rdx, [rsp + 38h]
    call eu4h_observe
    mov rcx, [rsp + 40h]
    mov rdx, [rsp + 48h]
    mov r8, [rsp + 50h]
    mov r9, [rsp + 58h]
    mov r10, [rsp + 20h]
    mov r11, [rsp + 28h]
    add rsp, 38h
    ret
eu4h_observer_entry ENDP
END
