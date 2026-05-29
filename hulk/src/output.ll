; ModuleID = 'hulk'
target triple = "x86_64-pc-linux-gnu"

declare ptr @malloc(i64)
declare i64 @strlen(ptr)
declare ptr @strcpy(ptr, ptr)
declare ptr @strcat(ptr, ptr)

declare i32 @printf(ptr, ...)
@.fmt_double = private unnamed_addr constant [4 x i8] c"%g\0A\00"
@.fmt_str    = private unnamed_addr constant [4 x i8] c"%s\0A\00"
@.str_true   = private unnamed_addr constant [5 x i8] c"true\00"
@.str_false  = private unnamed_addr constant [6 x i8] c"false\00"

define i32 @main() {
entry:
  %t0 = alloca double
  store double 5.0, ptr %t0
  %t1 = alloca double
  store double 0.0, ptr %t1
  br label %while_cond_0
while_cond_0:
  %t2 = load double, ptr %t0
  %t3 = fcmp oge double %t2, 0.0
  br i1 %t3, label %while_body_1, label %while_end_2
while_body_1:
  %t4 = load double, ptr %t0
  %t5 = call i32 (ptr, ...) @printf(ptr @.fmt_double, double %t4)
  %t6 = load double, ptr %t0
  %t7 = fsub double %t6, 1.0
  store double %t7, ptr %t0
  store double %t7, ptr %t1
  br label %while_cond_0
while_end_2:
  %t8 = load double, ptr %t1
  ret i32 0
}