#ifndef _SOS_
#define _SOS_ 1

#include <string.h>

#define MAX_MSG_SIZE 1024 * 1024

#define Public __attribute__((annotate("public")))
#define Private __attribute__((annotate("private")))

#define IPC_INPUT ((char *)0x900000)
// some size reserved for message header
#define IPC_OUTPUT (IPC_INPUT + MAX_MSG_SIZE)

typedef enum {
	Invalid,
	Int32,
	UInt32,
	Int64,
	UInt64,
	Float,
	Double,
	Error,
	String,
	Opaque,
	Function
} Type;

#define _TYPE_Int32 int
#define _TYPE_UInt32 unsigned int
#define _TYPE_Int64 long long
#define _TYPE_UInt64 unsigned long long
#define _TYPE_Float float
#define _TYPE_Double double

#define SOS_TYPE(name) _TYPE_##name

typedef struct {
	Type val_type;
	unsigned int length;
	char data[];
} Value;

struct values {
	unsigned int count;
	unsigned int current_offset;
	Value values[];
};

typedef struct values *Values;

// Read

#define NumValues(vals_ptr) vals_ptr->count

#define _Get_At_Offset_(vals)                                                  \
	((Value *)((char *)vals->values + vals->current_offset))

void *sos_get_data(Values vals, unsigned int *size);

#define GetValue(vals_ptr, val_type)                                           \
	(*(SOS_TYPE(val_type) *)sos_get_data((vals_ptr), NULL))

#define CopyValue(vals_ptr, dst)                                               \
	{                                                                          \
		int size = GetSize(vals_ptr);                                          \
		dst = alloca(size);                                                    \
		void *src = sos_get_data(vals_ptr, NULL);                              \
		memcpy((void *)dst, src, size);                                        \
	}

#define GetOpaque(vals_ptr, length_ptr) (sos_get_data(vals_ptr, length_ptr))

#define GetString(vals_ptr) (char *)sos_get_data(vals_ptr, NULL)

#define GetType(vals_ptr) (_Get_At_Offset_(vals_ptr)->type)
#define GetSize(vals_ptr) (_Get_At_Offset_(vals_ptr)->length)

#define IsError(vals_ptr) (GetType(vals_ptr) == Error)
#define GetError(vals_ptr) GetString(vals_ptr)

void GetFunction(Values vals_ptr, char **module, char **func);

// Write

#define vals_ipc ((Values)(IPC_OUTPUT + 512))
#define ClearValues()                                                          \
	*vals_ipc = (struct values) { 0, 0 }

void sos_set_data(Values vals, Type type, unsigned int size, const void *data);

#define AddValue(vals_ptr, itype, val_data)                                              \
	{                                                                          \
		SOS_TYPE(itype) tmp = (val_data);                                      \
		sos_set_data(vals_ptr, itype, sizeof(tmp), &tmp);                      \
	}

#define SetOpaque(vals_ptr, data_ptr, size) sos_set_data(vals_ptr, Opaque, size, data_ptr)

#define SetString(vals_ptr, str) sos_set_data(vals_ptr, String, strlen(str) + 1, str)

#define SetError(vals_ptr, str) sos_set_data(vals_ptr, Error, strlen(str) + 1, str)

void SetFunction(const char *module, const char *func);

#define ReturnValues() return vals_ipc
#define Call(module, name) sys_fuse(module, name, vals_ipc)

#endif
