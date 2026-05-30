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

@.str.35 = private unnamed_addr constant [5 x i8] c"Phil\00"
@.str.36 = private unnamed_addr constant [8 x i8] c"Collins\00"
%VTable_Person = type { ptr }
@vtable_Person = global %VTable_Person { ptr @Person_name }

%Person = type { ptr, ptr, ptr }

define ptr @Person_new(ptr %param_firstname, ptr %param_lastname) {
entry:
  %t0 = getelementptr %Person, ptr null, i32 1
  %t1 = ptrtoint ptr %t0 to i64
  %t2 = call ptr @malloc(i64 %t1)
  %t3 = getelementptr inbounds %Person, ptr %t2, i32 0, i32 0
  store ptr @vtable_Person, ptr %t3
  %t4 = alloca ptr
  store ptr %param_firstname, ptr %t4
  %t5 = alloca ptr
  store ptr %param_lastname, ptr %t5
  %t6 = load ptr, ptr %t4
  %t7 = getelementptr inbounds %Person, ptr %t2, i32 0, i32 1
  store ptr %t6, ptr %t7
  %t8 = load ptr, ptr %t5
  %t9 = getelementptr inbounds %Person, ptr %t2, i32 0, i32 2
  store ptr %t8, ptr %t9
  ret ptr %t2
}

define void @Person_init_fields(ptr %self, ptr %param_firstname, ptr %param_lastname) {
entry:
  %t10 = alloca ptr
  store ptr %param_firstname, ptr %t10
  %t11 = alloca ptr
  store ptr %param_lastname, ptr %t11
  %t12 = load ptr, ptr %t10
  %t13 = getelementptr inbounds %Person, ptr %self, i32 0, i32 1
  store ptr %t12, ptr %t13
  %t14 = load ptr, ptr %t11
  %t15 = getelementptr inbounds %Person, ptr %self, i32 0, i32 2
  store ptr %t14, ptr %t15
  ret void
}

define double @Person_name (ptr %self) {
entry:
  %t16 = getelementptr inbounds %Person, ptr %self, i32 0, i32 1  ; campo firstname
  %t17 = load ptr, ptr %t16
  %t18 = alloca ptr
  store ptr %t17, ptr %t18
  %t19 = getelementptr inbounds %Person, ptr %self, i32 0, i32 2  ; campo lastname
  %t20 = load ptr, ptr %t19
  %t21 = alloca ptr
  store ptr %t20, ptr %t21
  %t22 = getelementptr inbounds %Person, ptr %self, i32 0, i32 1
  %t23 = load double, ptr %t22
  ret double %t23
}

%VTable_Knight = type { ptr }
@vtable_Knight = global %VTable_Knight { ptr @Knight_name }

%Knight = type { ptr, ptr, ptr }

define ptr @Knight_new() {
entry:
  %t24 = getelementptr %Knight, ptr null, i32 1
  %t25 = ptrtoint ptr %t24 to i64
  %t26 = call ptr @malloc(i64 %t25)
  %t27 = getelementptr inbounds %Knight, ptr %t26, i32 0, i32 0
  store ptr @vtable_Knight, ptr %t27
  call void @Person_init_fields(ptr %t26)
  ret ptr %t26
}

define void @Knight_init_fields(ptr %self) {
entry:
  call void @Person_init_fields(ptr %self)
  ret void
}

define double @Knight_name (ptr %self) {
entry:
  %t28 = getelementptr inbounds %Knight, ptr %self, i32 0, i32 1  ; campo firstname
  %t29 = load ptr, ptr %t28
  %t30 = alloca ptr
  store ptr %t29, ptr %t30
  %t31 = getelementptr inbounds %Knight, ptr %self, i32 0, i32 2  ; campo lastname
  %t32 = load ptr, ptr %t31
  %t33 = alloca ptr
  store ptr %t32, ptr %t33
  ; base() → llamada directa a @Person_name (sin pasar por vtable)
  %t34 = call double @Person_name(ptr %self)
  ret double %t34
}

define i32 @main() {
entry:
  %t37 = call ptr @Knight_new(ptr @.str.35, ptr @.str.36)
  %t38 = alloca ptr
  store ptr %t37, ptr %t38
  %t39 = load ptr, ptr %t38
  ; Despacho virtual: Knight.name()
  %t40 = getelementptr inbounds %Knight, ptr %t39, i32 0, i32 0  ; leer vptr
  %t41 = load ptr, ptr %t40
  %t42 = getelementptr inbounds %VTable_Knight, ptr %t41, i32 0, i32 0  ; slot name
  %t43 = load ptr, ptr %t42
  %t44 = call double %t43(ptr %t39)
  %t45 = call i32 (ptr, ...) @printf(ptr @.fmt_double, double %t44)
  ret i32 0
}