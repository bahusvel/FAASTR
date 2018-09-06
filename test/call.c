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

long sys_fuse() {
  long r;
  asm("mov $0x3, %%rax;"
      "int $0x80"
      : "=a"(r)
      :);
  return r;
}

long sys_cast() {
  long r;
  asm("mov $0x4, %%rax;"
      "int $0x80"
      : "=a"(r)
      :);
  return r;
}


void call() {
  const char *hello = "calling";
  sys_write((void *)hello, 7);
  sys_fuse();
  sys_exit();
}
