; ModuleID = 'hulk'
target triple = "x86_64-pc-linux-gnu"

; declaración externa de malloc (para constructores)
declare ptr @malloc(i64)
declare i64 @strlen(ptr)
declare ptr @strcpy(ptr, ptr)
declare ptr @strcat(ptr, ptr)

; --- Nativas / Built-ins ---
declare i32 @printf(ptr, ...)
@.fmt_double = private unnamed_addr constant [4 x i8] c"%g\0A\00"

@.fmt_str = private unnamed_addr constant [4 x i8] c"%s\0A\00"
@.str_true = private unnamed_addr constant [5 x i8] c"true\00"
@.str_false = private unnamed_addr constant [6 x i8] c"false\00"

define i32 @main() {
entry:
  %t0 = call i32 (ptr, ...) @printf(ptr @.fmt_double, double 25)
  ret i32 0
}