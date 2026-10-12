#include "efi_sjlj.h"
int ash_main(int argc, char **argv);
int main(int argc, char **argv)
{
    return efi_sjlj_run(ash_main, argc, argv);
}
