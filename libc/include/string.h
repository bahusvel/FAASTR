/* string.h standard header*/
#ifndef STRING_H
#define STRING_H 1

#define NULL 0

#define size_t unsigned int

/*declarations*/
void * memchr(const void *s, int c, size_t n);
int memcmp(const void *s1, const void *s2, size_t n);
void * memcpy (void *dstpp, const void *srcpp, size_t len);
void * memmove (void *dstpp, const void *srcpp, size_t len);
void * memset (void *s, int c, size_t n);
char *strcat(char *s1, const char *s2);
char *strchr(const char *s, int c);
int strcmp(const char *s1,const char *s2);

char *strcpy(char *dest, const char *src);
size_t strcspn(const char *s, const char *reject);

size_t strlen (const char *s);
char *strncat(char *dest, const char *src, size_t n);
int strncmp(const char *s1, const char *s2, size_t n);
char *strncpy(char *dest, const char *src, size_t n);

//
char *strpbrk(const char *s1, const char *s2);
size_t (strspn)(const char *s1, const char *s2);
char *strrchr(const char *s, int c);
char *strstr (const char *s1, const char *s2);
char *strtok(char *s1, const char *s2);

//Denis did it
void reverse(char s[]);
int itoa(int n, char s[]);
static int intToStr(int x, char str[], int d);
int ipow(int base, int exp);
void ftoa(float n, char *res, int afterpoint);

#endif
