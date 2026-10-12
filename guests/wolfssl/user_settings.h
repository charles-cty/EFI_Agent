#ifndef EFI_WOLFSSL_USER_SETTINGS_H
#define EFI_WOLFSSL_USER_SETTINGS_H

#ifdef BUILDING_WOLFSSL
#include <arpa/inet.h>
#endif

/* Single-thread WASI guest. curl and WolfSSH install their own stream I/O. */
#define WOLFSSL_NO_OPTIONS_H
#define HAVE_LIMITS_H 1
#define HAVE_SYS_TIME_H 1
#define HAVE_SYS_UN_H 1
#define SINGLE_THREADED
#define WOLFSSL_USER_IO
#define WOLFSSL_NO_SOCK
#define NO_DEV_RANDOM
#define CUSTOM_RAND_GENERATE_BLOCK efi_wolfssl_random
int efi_wolfssl_random(unsigned char *output, unsigned int size);

#define NO_ASM
#define WOLFSSL_SP_MATH_ALL
/* Match CMake's WASI integer probe in every crypto-library consumer. */
#define HAVE___UINT128_T 1
#define NO_OLD_TLS
#define NO_RC4
#define NO_DES3
#define NO_DSA
#define NO_MD5
#define HAVE_ECC
#define ECC_TIMING_RESISTANT
#define WC_RSA_BLINDING
#define WC_RSA_PSS
#define HAVE_AESGCM
#define WOLFSSL_SHA384
#define WOLFSSL_SHA512
#define HAVE_HKDF
#define WOLFSSL_TLS13
#define HAVE_FFDHE_2048
#define HAVE_SUPPORTED_CURVES
#define HAVE_TLS_EXTENSIONS
#define HAVE_SNI
#define HAVE_ALPN
#define WOLFSSL_IP_ALT_NAME
#define WOLFSSL_ALT_CERT_CHAINS
#define OPENSSL_EXTRA
#define OPENSSL_ALL
#define HAVE_EX_DATA
#define WOLFSSL_WOLFSSH
#define WOLFSSL_AES_COUNTER
#define HAVE_AES_ECB

#endif
