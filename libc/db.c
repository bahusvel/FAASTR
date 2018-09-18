#include "alloca.h"
#include "db.h"
#include "runtime.h"
#include "string.h"

void set(string key, const void *value, unsigned int value_length) {
	ClearValues();
	SetString(key);
	SetOpaque((const char *)value, value_length);
	Call("db", "set");
}

void *get(string key, unsigned int *value_length) {
	ClearValues();
	SetString(key);
	Values dbGet = Call("db", "get");
	return (void *)GetOpaque(dbGet, value_length);
}

void delete (string key) {
	ClearValues();
	SetString(key);
	Call("db", "delete");
}

static void ListSetSize(string list_name, int size) {
	char *key = alloca(strlen(list_name) + strlen("index") + 2);
	strcpy(key, list_name);
	strcat(key, "/index");
	set(key, &size, sizeof(size));
}

void ListNew(string list_name) { ListSetSize(list_name, 0); }

int ListSize(string list_name) {
	char *key = alloca(strlen(list_name) + strlen("index") + 2);
	strcpy(key, list_name);
	strcat(key, "/index");
	unsigned int val_len;
	return *(int *)get(key, &val_len);
}

static void ListSetNoCheck(string list_name, int index, void *value,
						   int value_length) {
	if (index < 0)
		return;

	char number[20] = {0};
	itoa(index, number);
	char *key = alloca(strlen(list_name) + strlen(number) + 2);
	strcpy(key, list_name);
	strcat(key, "/");
	strcat(key, number);
	set(key, value, value_length);
}

void ListSet(string list_name, int index, void *value, int value_length) {
	int size = ListSize(list_name);
	if (index >= size)
		return;
	ListSetNoCheck(list_name, index, value, value_length);
}

void ListAppend(string list_name, int index, void *value, int value_length) {
	int listSize = ListSize(list_name);
	ListSetNoCheck(list_name, listSize + 1, value, value_length);
	ListSetSize(list_name, listSize + 1);
}

void *ListGet(string list_name, int index, unsigned int *value_length) {
	if (index < 0)
		return NULL;
	int size = ListSize(list_name);
	if (index >= size)
		return NULL;
	char number[20] = {0};
	itoa(index, number);
	char *key = alloca(strlen(list_name) + strlen(number) + 2);
	strcpy(key, list_name);
	strcat(key, "/");
	strcat(key, number);
	return get(key, value_length);
}

void ListDelete(string list_name) {
	char number[20] = {0};
	char *key = alloca(strlen(list_name) + strlen(number) + 2);
	strcpy(key, list_name);
	strcat(key, "/");
	char *numOffset = key + strlen(key);
	int size = ListSize(list_name);
	for (int i = 0; i < size; i++) {
		itoa(i, number);
		strcpy(numOffset, number);
		delete (key);
	}
	strcpy(numOffset, "index");
	delete (key);
}
