#include <wasi/api.h>
#include <wolfssl/wolfcrypt/error-crypt.h>
#include "user_settings.h"

int efi_wolfssl_random(unsigned char *output, unsigned int size)
{
    return __wasi_random_get(output, size) == 0 ? 0 : RNG_FAILURE_E;
}
