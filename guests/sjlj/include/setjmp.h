#ifndef EFI_SJLJ_SETJMP_H
#define EFI_SJLJ_SETJMP_H

#include <stdint.h>

/* This header requires the guest Asyncify transform and efi_sjlj_run driver.
 * Environments identify saved Wasm frames; linear-memory objects are not
 * rolled back. Do not jump to a function which has already returned.
 */
typedef uintptr_t jmp_buf[1];
typedef jmp_buf sigjmp_buf;

__attribute__((noinline, returns_twice)) int efi_sjlj_setjmp(jmp_buf environment);
__attribute__((noinline)) void efi_sjlj_longjmp(jmp_buf environment, int value);

#define setjmp(environment) efi_sjlj_setjmp(environment)
#define longjmp(environment, value) efi_sjlj_longjmp(environment, value)
#define _setjmp(environment) efi_sjlj_setjmp(environment)
#define _longjmp(environment, value) efi_sjlj_longjmp(environment, value)
/* Guest processes have no asynchronous POSIX signals or signal masks. */
#define sigsetjmp(environment, save_mask) efi_sjlj_setjmp(environment)
#define siglongjmp(environment, value) efi_sjlj_longjmp(environment, value)

#endif
