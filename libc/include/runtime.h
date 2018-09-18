#ifndef _RUNTIME_
#define _RUNTIME_ 1

#include "sos.h"

unsigned long rand();

void *malloc(long size);

void *calloc(long count, long elem_size);

void free(void *ptr);

void *realloc(void *ptr, long size);

/*
static long time() {
	return call((func){"runtime", "time"}, 0);
}

static void log(char *entry) {
	call((func){"runtime", "rand"}, (long)entry);
}
*/

#endif
