#ifndef EFI_SJLJ_H
#define EFI_SJLJ_H

/* Call the transformed application through this driver. It stays below the
 * saved frames and handles each unwind/rewind without leaving the module.
 */
int efi_sjlj_run(int (*entry)(int, char **), int argc, char **argv);

#endif
