#include <sos.h>

void sys_exit() {
  asm("mov $1, %rax;"
      "xor %rbx, %rbx;"
      "int $0x80");
}

long sys_return(Values ptr, long length) {
  long r;
  asm("mov $0x6, %%rax;"
      "int $0x80"
      : "=a"(r)
      : "b"(ptr), "c"(length)
      );
  return r;
}

Values sys_write(Values ptr, long len) {
  long r;
  asm("mov $0x2, %%rax;"
      "int $0x80"
      : "=a"(r)
      : "b"(ptr), "c"(len));
  return (Values)r;
}

Values sys_fuse(Values ptr, long length) {
  long r;
  asm("mov $0x3, %%rax;"
      "int $0x80"
      : "=a"(r)
      : "b"(ptr), "c"(length)
      );
  return (Values)r;
}

long sys_cast(Values ptr, long length) {
  long r;
  asm("mov $0x4, %%rax;"
      "int $0x80"
      : "=a"(r)
      :"b"(ptr), "c"(length));
  return r;
}

void print(Values args) {
  char buf[4096] = {0};
  Values vals = (Values)buf;
  const char* input = GetString(args);
  SetString(vals, input);
  sys_write(vals, 4096);
  sys_return(vals, 4096);
}

void passthrough(Values args) {
  char buf[4096] = {0};
  Values vals = (Values)buf;
  const char* input = GetString(args);
  SetString(vals, input);
  sys_return(vals, 4096);
}

void call() {
  const char *hello = "calling";
  char buf[4096] = {0};
  Values vals = (Values)buf;
  SetFunction(vals, "call", "passthrough");
  SetString(vals, hello);

  Values pass_out = sys_fuse(vals, 4096);
  SetString(vals, GetString(pass_out)); 

  sys_return(pass_out, 4096);
}
