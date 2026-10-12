#include "Python.h"
#include <stdio.h>
#include <string.h>
#include "encodings.h"
#include "encodings_aliases.h"
#include "encodings_utf_8.h"
#include "efi.h"

PyMODINIT_FUNC PyInit__efi_shell(void);

static const struct _frozen frozen[] = {
    {"encodings", _Py_M__encodings, sizeof(_Py_M__encodings), 1},
    {"encodings.aliases", _Py_M__encodings_aliases, sizeof(_Py_M__encodings_aliases), 0},
    {"encodings.utf_8", _Py_M__encodings_utf_8, sizeof(_Py_M__encodings_utf_8), 0},
    {"efi", _Py_M__efi, sizeof(_Py_M__efi), 0},
    {NULL, NULL, 0, 0}
};

static void check(PyStatus status)
{
    if (PyStatus_Exception(status)) {
        Py_ExitStatusException(status);
    }
}

int main(int argc, char **argv)
{
    if (argc != 3 || strcmp(argv[1], "-e") != 0) {
        fputs("Usage: python -e CODE\n", stderr);
        return 2;
    }
    if (PyImport_AppendInittab("_efi_shell", PyInit__efi_shell) != 0) {
        fputs("Cannot register the shell module\n", stderr);
        return 1;
    }
    PyImport_FrozenModules = frozen;
    PyConfig config;
    PyConfig_InitIsolatedConfig(&config);
    config.use_environment = 0;
    config.site_import = 0;
    config.write_bytecode = 0;
    config.install_signal_handlers = 0;
    config.parse_argv = 0;
    config.module_search_paths_set = 1;
    check(PyConfig_SetString(&config, &config.program_name, L"python"));
    check(PyConfig_SetString(&config, &config.executable, L"python"));
    check(PyConfig_SetString(&config, &config.home, L"."));
    check(PyConfig_SetString(&config, &config.stdio_encoding, L"utf-8"));
    check(PyConfig_SetString(&config, &config.stdio_errors, L"replace"));
    check(PyConfig_SetString(&config, &config.filesystem_encoding, L"utf-8"));
    check(PyConfig_SetString(&config, &config.filesystem_errors, L"surrogateescape"));
    check(PyConfig_SetBytesArgv(&config, argc, argv));
    PyStatus initialized = Py_InitializeFromConfig(&config);
    PyConfig_Clear(&config);
    check(initialized);
    // File imports use only built-in and frozen modules. User file operations
    // go through efi.shell; _io retains standard-stream support only.
    PyObject *builtins = PyEval_GetBuiltins();
    if (PyDict_DelItemString(builtins, "open") != 0 ||
        PyDict_DelItemString(builtins, "input") != 0 ||
        PyDict_DelItemString(builtins, "breakpoint") != 0) {
        PyErr_Print();
        Py_FinalizeEx();
        return 1;
    }
    int status = PyRun_SimpleString(argv[2]) == 0 ? 0 : 1;
    if (Py_FinalizeEx() < 0) {
        return 120;
    }
    return status;
}
