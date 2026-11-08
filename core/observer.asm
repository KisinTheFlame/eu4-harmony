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

.code
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
