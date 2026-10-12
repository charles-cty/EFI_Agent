	.globaltype __stack_pointer, i32
	.functype efi_sjlj_get_sp () -> (i32)
	.functype efi_sjlj_set_sp (i32) -> ()
	.section .text.efi_sjlj_get_sp,"",@
	.globl efi_sjlj_get_sp
efi_sjlj_get_sp:
	.functype efi_sjlj_get_sp () -> (i32)
	global.get __stack_pointer
	end_function
	.section .text.efi_sjlj_set_sp,"",@
	.globl efi_sjlj_set_sp
efi_sjlj_set_sp:
	.functype efi_sjlj_set_sp (i32) -> ()
	local.get 0
	global.set __stack_pointer
	end_function
