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

void hellohelloexit() {
  const char *hello = "hello";
  sys_write((void *)hello, 5);
  sys_write((void *)hello, 5);
  sys_exit();
}
