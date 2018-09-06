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

void hello() {
  const char *hello = "hello from ivshmem";
  sys_write((void *)hello, 16);
  sys_exit();
}
