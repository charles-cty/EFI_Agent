#include "efi_process.h"
#include "efi_shell.h"
#include "efi_wasi.h"

int efi_shell_done(const struct efi_shell_task *task)
{
    return task->pid == 0 && task->output[0].descriptor < 0 &&
        task->output[1].descriptor < 0;
}

int32_t efi_shell_start(struct efi_shell_task *task, const char *const argv[],
    uint32_t argc, char *scratch, uint32_t scratch_size,
    char *stdout_bytes, uint32_t stdout_capacity,
    char *stderr_bytes, uint32_t stderr_capacity)
{
    *task = (struct efi_shell_task){
        .output = {{stdout_bytes, stdout_capacity, 0, -1},
                   {stderr_bytes, stderr_capacity, 0, -1}}
    };
    uint32_t pipes[3][2];
    uint32_t created = 0;
    int32_t result;
    for (; created < 3; ++created) {
        result = efi_process_pipe(pipes[created]);
        if (result < 0) {
            goto cleanup;
        }
    }
    result = efi_process_spawn_argv_fds(argv, argc, scratch, scratch_size,
        pipes[0][0], pipes[1][1], pipes[2][1]);
    if (result < 0) {
        goto cleanup;
    }
    task->pid = (uint32_t)result;
    efi_wasi_close(pipes[0][0]);
    efi_wasi_close(pipes[0][1]);
    for (uint32_t stream = 0; stream < 2; ++stream) {
        efi_wasi_close(pipes[stream + 1][1]);
        task->output[stream].descriptor = (int32_t)pipes[stream + 1][0];
    }
    return 0;

cleanup:
    for (uint32_t index = 0; index < created; ++index) {
        efi_wasi_close(pipes[index][0]);
        efi_wasi_close(pipes[index][1]);
    }
    task->error = result;
    return result;
}

static void drain(struct efi_shell_task *task, uint32_t stream)
{
    struct efi_shell_output *output = &task->output[stream];
    char chunk[4096];
    struct efi_wasi_iovec vector = {chunk, sizeof chunk};
    uint32_t received;
    int32_t error = efi_wasi_read((uint32_t)output->descriptor, &vector, 1, &received);
    if (error != 0 || received == 0) {
        if (error != 0 && task->error == 0) {
            task->error = -error;
        }
        efi_wasi_close((uint32_t)output->descriptor);
        output->descriptor = -1;
        return;
    }
    uint32_t available = output->capacity - output->size;
    uint32_t saved = received < available ? received : available;
    for (uint32_t index = 0; index < saved; ++index) {
        output->bytes[output->size++] = chunk[index];
    }
    if (saved < received && task->error == 0) {
        task->error = -27; /* EFBIG */
    }
}

int32_t efi_shell_step(struct efi_shell_task *const tasks[], uint32_t count, int wait)
{
    if (count > EFI_SHELL_MAX_TASKS) {
        return -7;
    }
    struct efi_wasi_subscription subscriptions[EFI_SHELL_MAX_TASKS * 2 + 1] = {0};
    struct efi_wasi_event events[EFI_SHELL_MAX_TASKS * 2 + 1];
    uint32_t used = 0;
    for (uint32_t index = 0; index < count; ++index) {
        struct efi_shell_task *task = tasks[index];
        if (task->pid != 0) {
            int32_t result = efi_process_waitpid_nohang(task->pid, &task->status);
            if (result < 0) {
                return result;
            }
            if (result > 0) {
                task->pid = 0;
            } else if (task->output[0].descriptor < 0 || task->output[1].descriptor < 0) {
                // An exited child may close the last pipe while a sibling is
                // blocked. Poll must return so that exit checks run again.
                wait = 0;
            }
        }
        for (uint32_t stream = 0; stream < 2; ++stream) {
            if (task->output[stream].descriptor >= 0) {
                subscriptions[used].userdata = index * 2 + stream;
                subscriptions[used].type = 1;
                subscriptions[used++].value.fd = (uint32_t)task->output[stream].descriptor;
            }
        }
    }
    if (used == 0) {
        return -efi_wasi_yield();
    }
    if (!wait) {
        subscriptions[used].type = 0;
        subscriptions[used++].value.clock.timeout = 0;
    }
    uint32_t received;
    int32_t error = efi_wasi_poll(subscriptions, events, used, &received);
    if (error != 0) {
        return -error;
    }
    for (uint32_t index = 0; index < received; ++index) {
        const struct efi_wasi_event *event = &events[index];
        if (event->type == 0) {
            continue;
        }
        struct efi_shell_task *task = tasks[event->userdata / 2];
        uint32_t stream = (uint32_t)(event->userdata % 2);
        if (event->error != 0) {
            if (task->error == 0) {
                task->error = -(int32_t)event->error;
            }
            efi_wasi_close((uint32_t)task->output[stream].descriptor);
            task->output[stream].descriptor = -1;
        } else {
            drain(task, stream);
        }
    }
    return 0;
}
