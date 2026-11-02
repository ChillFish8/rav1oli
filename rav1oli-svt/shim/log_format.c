#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>

#include "EbSvtAv1Enc.h"
#include "log_format.h"

#define STACK_MESSAGE_BYTES 1024

static rav1oli_svt_log_sink installed_sink;

static void format_and_forward(void *context, SvtAv1LogLevel level, const char *tag, const char *format,
                               va_list args) {
    (void)context;

    va_list retry_args;
    va_copy(retry_args, args);

    char stack_message[STACK_MESSAGE_BYTES];
    int  length = vsnprintf(stack_message, sizeof(stack_message), format, args);
    if (length < 0) {
        va_end(retry_args);
        return;
    }

    if ((size_t)length < sizeof(stack_message)) {
        va_end(retry_args);
        installed_sink((int)level, tag, stack_message);
        return;
    }

    char *heap_message = malloc((size_t)length + 1);
    if (heap_message == NULL) {
        va_end(retry_args);
        installed_sink((int)level, tag, stack_message);
        return;
    }

    vsnprintf(heap_message, (size_t)length + 1, format, retry_args);
    va_end(retry_args);
    installed_sink((int)level, tag, heap_message);
    free(heap_message);
}

void rav1oli_svt_install_log(rav1oli_svt_log_sink sink) {
    installed_sink = sink;
    svt_av1_set_log_callback(format_and_forward, NULL);
}
