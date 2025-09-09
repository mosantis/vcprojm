#include "string_utils.h"

int string_length(const char* str) {
    int len = 0;
    while (*str++) len++;
    return len;
}