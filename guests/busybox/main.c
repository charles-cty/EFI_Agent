#include "efi_sjlj.h"
#include "busybox.h"
int ash_main(int argc, char **argv);
int main(int argc, char **argv)
{
    if (argc > 0 && strcmp(bb_basename(argv[0]), "ash") != 0
        && strcmp(bb_basename(argv[0]), "sh") != 0)
        return efi_sjlj_run(efi_busybox_applet, argc, argv);
    return efi_sjlj_run(ash_main, argc, argv);
}
