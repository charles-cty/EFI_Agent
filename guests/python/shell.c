#include "Python.h"
#include "efi_shell.h"
#include <string.h>

#define OUTPUT_LIMIT (512 * 1024)

static PyObject *run_command(PyObject *self, PyObject *argument)
{
    (void)self;
    if (!PyUnicode_Check(argument)) {
        PyErr_SetString(PyExc_TypeError, "Expected shell text");
        return NULL;
    }
    Py_ssize_t length;
    const char *text = PyUnicode_AsUTF8AndSize(argument, &length);
    if (text == NULL) {
        return NULL;
    }
    if (length > 65520 || memchr(text, 0, (size_t)length) != NULL) {
        PyErr_SetString(PyExc_ValueError, "Command contains NUL or exceeds the argument limit");
        return NULL;
    }
    char *output = PyMem_Malloc(OUTPUT_LIMIT);
    char *error = PyMem_Malloc(OUTPUT_LIMIT);
    char *scratch = PyMem_Malloc(65536);
    PyObject *result = NULL;
    if (output == NULL || error == NULL || scratch == NULL) {
        PyErr_NoMemory();
        goto cleanup;
    }
    struct efi_shell_task task;
    const char *arguments[] = {"ash", "-c", text};
    int32_t failure = efi_shell_start(&task, arguments, 3, scratch, 65536,
        output, OUTPUT_LIMIT, error, OUTPUT_LIMIT);
    if (failure != 0) {
        PyErr_Format(PyExc_RuntimeError, "Cannot start ash: %d", failure);
        goto cleanup;
    }
    struct efi_shell_task *tasks[] = {&task};
    while (!efi_shell_done(&task)) {
        failure = efi_shell_step(tasks, 1, 1);
        if (failure != 0) {
            // A dispatcher/protocol failure is unrecoverable here. Do not
            // return with an active child and unowned capture descriptors.
            Py_FatalError("Shell capture dispatcher failed");
        }
    }
    if (task.error != 0) {
        PyErr_Format(PyExc_RuntimeError, "Shell capture failed: %d", task.error);
        goto cleanup;
    }
    result = Py_BuildValue("(iy#y#)", task.status,
        output, (Py_ssize_t)task.output[0].size,
        error, (Py_ssize_t)task.output[1].size);

cleanup:
    PyMem_Free(scratch);
    PyMem_Free(error);
    PyMem_Free(output);
    return result;
}

static PyMethodDef methods[] = {
    {"run", run_command, METH_O, "Run fresh ash and return status, stdout bytes and stderr bytes."},
    {NULL, NULL, 0, NULL}
};

static struct PyModuleDef module = {
    PyModuleDef_HEAD_INIT,
    "_efi_shell",
    "Cooperative guest shell capture.",
    -1,
    methods,
    NULL, NULL, NULL, NULL
};

PyMODINIT_FUNC PyInit__efi_shell(void)
{
    return PyModule_Create(&module);
}
