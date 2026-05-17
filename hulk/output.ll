; ModuleID = 'hulk'
target triple = 'x86_64-pc-linux-gnu'

define i32 @main() {
entry:
  %t0 = fsub double 2, 1
  %t1 = fmul double 3, %t0
  %t2 = fadd double 5, %t1
  %t3 = alloca double
  store double %t2, ptr %t3
  %t4 = load double, ptr %t3
  %t5 = fadd double %t4, 1
  ret i32 0
}