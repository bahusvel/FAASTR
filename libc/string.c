#include <string.h>

/*memchr function*/
/*
**Description:
**The memchr() function scans the initial n bytes of the memory area
**pointed to by s for the first instance of c.  Both c and the bytes of
**the memory area pointed to by s are interpreted as unsigned char.
**RETURN VALUE:
**The memchr() and memrchr() functions return a pointer to the matching
**byte or NULL if the character does not occur in the given memory area.
*/
void *memchr(const void *s, int c, size_t n) {
	unsigned char uc = (unsigned char)c;
	unsigned char *su = (unsigned char *)s;
	for (; 0 < n; ++su, --n) {
		if (*su == uc)
			return ((void *)su);
	}
	return (NULL);
}

/*memcpy function*/
/*
**Description:
**The memcpy() function copies n bytes from memory srcpp to memory
**dstpp.The memory areas must not overlap.  Use memmove() if the
**memory areas do overlap.
**RETURN VALUE:
**The memcpy() function returns a pointer to dstpp.
*/
void *memcpy(void *dstpp, const void *srcpp, size_t len) {
	char *s1 = (char *)dstpp;
	char *s2 = (char *)srcpp;
	for (; 0 < len; ++s1, ++s2, --len) {
		*s1 = *s2;
	}
	return (dstpp);
}

/*memcmp function*/
/*
**Description:
**The memcmp() function compares the first n bytes (each interpreted as
**unsigned char) of the memory areas s1 and s2.
**RETURN VALUE:
**The memcmp() function returns an integer less than, equal to, or
**greater than zero if the first n bytes of s1 is found, respectively,
**to be less than, to match, or be greater than the first n bytes of s2.
**For a nonzero return value, the sign is determined by the sign of the
**difference between the first pair of bytes (interpreted as unsigned
**char) that differ in s1 and s2.If n is zero, the return value is zero.
*/
int memcmp(const void *s1, const void *s2, size_t n) {
	unsigned char *su1 = (unsigned char *)s1;
	unsigned char *su2 = (unsigned char *)s2;
	for (; 0 < n; ++su1, ++su2, --n) {
		if (*su1 != *su2)
			return ((*su1 < *su2) ? (-1) : (1));
	}
	return (0);
}

/*memmove function*/
/*
**Description:
**The memmove() function copies len bytes from memory area srcpp to memory
**area dstpp.  The memory areas may overlap: copying takes place as
**though the bytes in srcpp are first copied into a temporary array that
**does not overlap srcpp or dstpp, and the bytes are then copied from the
**temporary array to dstpp.
**RETURN VALUE:
**The memmove() function returns a pointer to dstpp.
*/
void *memmove(void *dstpp, const void *srcpp, size_t len) {
	char *sc1 = (char *)dstpp;
	char *sc2 = (char *)srcpp;
	if ((sc2 < sc1) && (sc1 < sc2 + len)) {
		for (sc1 += len, sc2 += len; 0 < len; --len) {
			*--sc1 = *--sc2;
		}
	} else {
		for (; 0 < len; --len) {
			*--sc1 = *--sc2;
		}
	}
	return (dstpp);
}

/*memset function*/
/*
**Description:
**The memset() function fills the first n bytes of the memory area
**pointed to by s with the constant byte c.
**RETURN VALUE:
**The memset() function returns a pointer to the memory area s.
*/
void *memset(void *s, int c, size_t n) {
	unsigned char uc = (unsigned char)c;
	unsigned char *su = s;
	for (; 0 < n; ++su, --n) {
		*su = uc;
	}
	return (s);
}

/*strncat function*/
/*
**Description:
**appends the string pointed to by src to the end of the string pointed to by
**dest up to n characters long.
**RETURN VALUE:
**This function returns a pointer to the resulting string dest.
*/
char *strncat(char *dest, const char *src, size_t n) {
	size_t dest_len = strlen(dest);
	size_t i;

	for (i = 0; i < n && src[i] != '\0'; i++)
		dest[dest_len + i] = src[i];
	dest[dest_len + i] = '\0';

	return dest;
}

/*strncmp function*/
/*
**Description:
**The strncmp()compares at most the first n bytes of s1 and s2.
**RETURN VALUE:
**This function return values that are as follows:
**if Return value < 0 then it indicates s1 is less than s2.
**if Return value > 0 then it indicates s2 is less than s1.
**if Return value = 0 then it indicates s1 is equal to s2.
*/
int strncmp(const char *s1, const char *s2, size_t n) {
	for (; 0 < n; ++s1, ++s2, --n) {
		if (*s1 != *s2) {
			return ((*(unsigned char *)s1) < (*(unsigned char *)s2) ? (-1)
																	: (1));
		} else if (*s1 == '\0') {
			return (0);
		}
	}
	return (0);
}

/*strncpy function*/
/*
**Description:
**copies up to n characters from the string pointed to, by src to dest.
**In a case where the length of src is less than that of n,
**the remainder of dest will be padded with null bytes.
**RETURN VALUE:
**This function returns the point of the final copy of the copied string.
*/
char *strncpy(char *dest, const char *src, size_t n) {
	size_t i;

	for (i = 0; i < n && src[i] != '\0'; i++)
		dest[i] = src[i];
	for (; i < n; i++)
		dest[i] = '\0';

	return dest;
}

/*strcat function*/
/*
**Description:
**appends the string pointed to by s1 to the end of the string pointed to by
**s2 up to n characters long.
**RETURN VALUE:
**This function returns a pointer to the resulting string s1.
*/
char *strcat(char *s1, const char *s2) {
	char *s = s1;
	for (; *s != '\0'; ++s)
		;
	for (; (*s = *s2) != '\0'; ++s, ++s2)
		;
	return s1;
}

/*strncmp function*/
/*
**Description:
**The strncmp()compares at most the first n bytes of s1 and s2.
**RETURN VALUE:
**This function return values that are as follows:
**if Return value < 0 then it indicates s1 is less than s2.
**if Return value > 0 then it indicates s2 is less than s1.
**if Return value = 0 then it indicates s1 is equal to s2.
*/
int strcmp(const char *s1, const char *s2) {
	for (; *s1 == *s2; ++s1, ++s2) {
		if (*s1 == '\0') {
			return (0);
		}
	}
	return ((*(unsigned char *)s1) < (*(unsigned char *)s2) ? (-1) : (1));
}

/*strcpy function*/
/*
**Description:
**The strcpy() function copies the string pointed to by src,
**including the terminating null byte ('\0'),to the buffer pointed to by dest
**RETURN VALUE:
**This function return values that are as follows:
**The strcpy()functions return a pointer to the destination string dest.
*/
char *strcpy(char *dest, const char *src) {
	char *s = dest;
	for (; (*s++ = *src++) != '\0';)
		;
	return (dest);
}

/*strlen function*/
/*
**Description:
**The strlen() function calculates the length of the string s, excluding the
**terminating null byte
**RETURN VALUE:
**The strlen() function returns the number of bytes in the string s
*/
size_t strlen(const char *s) {
	const char *sc;
	for (sc = s; *sc != '\0'; ++sc)
		;
	return (sc - s);
	;
}

/*strchr function*/
/*
**Description:
**strchr function returns a pointer to the first occurrence of the character
**c in the string s.
**terminating null byte
**RETURN VALUE:
**The strchr() functions return a pointer to the matched character or NULL
**if the character is not found.
*/
char *strchr(const char *s, int c) {
	const char ch = (char)c;
	for (; *s != ch; ++s)
		if (*s == '\0')
			return (NULL);
	return ((char *)s);
}

/*strcspn function*/
/*
**Description:
**The strcspn() function calculates the length of the initial segment of s1
**which consists entirely of bytes not in s2.
**RETURN VALUE:
**The strcspn() function returns the number of bytes in the initial segment
**of s1 which are not in the string s2.
*/
size_t strcspn(const char *s1, const char *s2) {
	const char *sc1;
	const char *sc2;

	for (sc1 = s1; *sc1 != '\0'; ++sc1) {
		for (sc2 = s2; *sc2 != '\0'; ++sc2) {
			if (*sc1 == *sc2) {
				return (sc1 - s1);
			}
		}
	}
	return (sc1 - s1);
}

/*strpbrk function*/
char *strpbrk(const char *s1, const char *s2) {
	const char *sc1, *sc2;
	for (sc1 = s1; *sc1 != '\0'; ++sc1)
		for (sc2 = s2; *sc2 != '\0'; ++sc2)
			if (*sc1 == *sc2)
				return ((char *)sc1);
	return (NULL);
}

/*strspn function*/

size_t(strspn)(const char *s1, const char *s2) {
	const char *sc1, *sc2;
	for (sc1 = s1; *sc1 != '\0'; ++sc1) {
		for (sc2 = s2;; ++sc2) {
			if (*sc2 == '\0')
				return (sc1 - s1);
			else if (*sc1 == *sc2)
				break;
		}
	}
	return (sc1 - s1);
}

/*strrchr function*/
char *strrchr(const char *s, int c) {
	const char ch = c;
	const char *sc;
	for (sc = NULL;; ++s) {
		if (*s == ch)
			sc = s;
		if (*s == '\0')
			return ((char *)sc);
	}
}

/*strstr function*/
char *strstr(const char *s1, const char *s2) {
	if (*s2 == '\0')
		return ((char *)s1);
	for (; (s1 = strchr(s1, *s2)) != NULL; ++s1) { /* match rest of prefix */
		const char *sc1, *sc2;
		for (sc1 = s1, sc2 = s2;;) {
			if (*++sc2 == '\0')
				return ((char *)s1);
			else if (*++sc1 != *sc2)
				break;
		}
	}
	return (NULL);
}

/*strtok function*/
char *strtok(char *s1, const char *s2) {
	char *sbegin, *send;
	static char *ssave = "";
	sbegin = s1 ? s1 : ssave;
	sbegin += strspn(sbegin, s2);
	if (*sbegin == '\0') {
		ssave = "";
		return (NULL);
	}
	send = sbegin + strcspn(sbegin, s2);
	if (*send != '\0')
		*send++ = '\0';
	ssave = send;
	return (sbegin);
}

void reverse(char s[]) {
	int i, j;
	char c;

	for (i = 0, j = strlen(s) - 1; i < j; i++, j--) {
		c = s[i];
		s[i] = s[j];
		s[j] = c;
	}
}

int itoa(int n, char s[]) {
	int i, sign;

	if ((sign = n) < 0) /* record sign */
		n = -n;			/* make n positive */
	i = 0;
	do {					   /* generate digits in reverse order */
		s[i++] = n % 10 + '0'; /* get next digit */
	} while ((n /= 10) > 0);   /* delete it */
	if (sign < 0)
		s[i++] = '-';
	s[i] = '\0';
	reverse(s);
	return i;
}

static int intToStr(int x, char str[], int d) {
	int i = 0;
	while (x) {
		str[i++] = (x % 10) + '0';
		x = x / 10;
	}

	// If number of digits required is more, then
	// add 0s at the beginning
	while (i < d)
		str[i++] = '0';

	reverse(str);
	str[i] = '\0';
	return i;
}

int ipow(int base, int exp) {
	int result = 1;
	while (exp) {
		if (exp & 1)
			result *= base;
		exp >>= 1;
		base *= base;
	}

	return result;
}

void ftoa(float n, char *res, int afterpoint) {
	// Extract integer part
	int ipart = (int)n;

	// Extract floating part
	float fpart = n - (float)ipart;

	// convert integer part to string
	int i = intToStr(ipart, res, 0);

	// check for display option after point
	if (afterpoint != 0) {
		res[i] = '.'; // add dot

		// Get the value of fraction part upto given no.
		// of points after dot. The third parameter is needed
		// to handle cases like 233.007
		fpart = fpart * ipow(10, afterpoint);

		intToStr((int)fpart, res + i + 1, afterpoint);
	}
}
