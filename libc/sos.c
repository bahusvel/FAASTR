#include <sos.h>
#include <string.h>

void *sos_get_data(Values vals, unsigned int *size) {
	Value *val = _Get_At_Offset_(vals);
	vals->current_offset += val->length + sizeof(Value);
	if (size)
		*size = val->length;
	return val->data;
}

void sos_set_data(Values vals, Type type, unsigned int size, const void *data) {
	Value *val = _Get_At_Offset_(vals);
	*val = (Value){type, size};
	memcpy(val->data, data, size);
	vals->count++;
	vals->current_offset += size + sizeof(Value);
}

void SetFunction(const char *module, const char *func) {
	long len_module = strlen(module);
	long len_func = strlen(func);
	unsigned int str_size = len_module + len_func + 2;

	Value *val = _Get_At_Offset_(vals_ipc);
	*val = (Value){Function, str_size};

	memcpy(val->data, module, len_module + 1);
	memcpy(val->data + len_module + 1, func, len_func + 1);

	vals_ipc->count++;
	vals_ipc->current_offset += str_size + sizeof(Value);
}

void GetFunction(Values vals_ptr, char **module, char **func) {
	char *data = (char *)sos_get_data(vals_ptr, NULL);
	*module = data;
	*func = data + strlen(data) + 1;
}
