void sys_exit() {
  asm("mov $1, %rax;"
      "xor %rbx, %rbx;"
      "int $0x80");
}

long sys_write(void *buf, long len) {
  long r;
  asm("mov $0x21000004, %%rax;"
      "int $0x80"
      : "=a"(r)
      : "b"(buf), "c"(len));
  return r;
}

void _start() {
  const char *hello = "hello\n";
  // sys_write((void *)hello, 6);
  sys_exit();
}
