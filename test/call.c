#include <sos.h>

void sys_exit() {
  asm("mov $1, %rax;"
      "xor %rbx, %rbx;"
      "int $0x80");
}

long sys_write(void *buf, long len) {
  long r;
  asm("mov $0x2, %%rax;"
      "int $0x80"
      : "=a"(r)
      : "b"(buf), "c"(len));
  return r;
}

long sys_fuse(Values ptr, long length) {
  long r;
  asm("mov $0x3, %%rax;"
      "int $0x80"
      : "=a"(r)
      : "b"(ptr), "c"(length)
      );
  return r;
}

long sys_cast(Values ptr, long length) {
  long r;
  asm("mov $0x4, %%rax;"
      "int $0x80"
      : "=a"(r)
      :"b"(ptr), "c"(length));
  return r;
}


void call() {
  const char *hello = "calling";
  sys_write((void *)hello, 7);
  char buf[4096] = {0};
  Values vals = (Values)buf;
  vals->count = 1;
  SetString(vals, hello);
  sys_fuse(vals, 4096);
  sys_exit();
}
