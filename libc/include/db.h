#ifndef _RUNTIME_DB_
#define _RUNTIME_DB_ 1

#include "runtime.h"
typedef const char *string;

void set(string key, const void *value, unsigned int value_length);
void *get(string key, unsigned int *value_length);
void *ListGet(string list_name, int index, unsigned int *value_length);

#endif
