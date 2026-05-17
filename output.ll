; ModuleID = 'hulk'
target triple = 'x86_64-pc-linux-gnu'

define i32 @main() {
entry:
  %t0 = fadd double 1, 1
  %t1 = fadd double 2, 1
  %t2 = xor i1 1, 1
  %t3 = zext i1 %t2 to i32
  ret i32 %t3
}