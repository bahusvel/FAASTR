#ifndef _ALLOCA_H
#define _ALLOCA_H 1

#undef alloca

#define alloca(size) __builtin_alloca(size)

#endif
