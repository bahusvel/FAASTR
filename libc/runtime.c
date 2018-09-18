#include <runtime.h>

unsigned long rand() {
	ClearValues();
	Values r1 = Call("runtime", "rand");
	return GetValue(r1, UInt64);
}

void *malloc(long size) {
	ClearValues();
	AddValue(Int64, size);
	Values r1 = Call("rt", "malloc");
	return (void *)GetValue(r1, Int64);
}

void *calloc(long count, long elem_size) {
	char *mem = (char *)malloc(count * elem_size);
	for (long i = 0; i < count * elem_size; i++)
		mem[i] = 0;
	return (void *)mem;
}

void free(void *ptr) {
	ClearValues();
	AddValue(Int64, (long)ptr);
	Call("rt", "free");
}

void *realloc(void *ptr, long size) {
	free(ptr);
	return malloc(size);
}
