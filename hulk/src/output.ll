; ModuleID = 'hulk'
target triple = "x86_64-pc-linux-gnu"

; declaración externa de malloc (para constructores)
declare ptr @malloc(i64)

%Point = type { double, double }

define ptr @Point_new() {
entry:
  %t0 = getelementptr %Point, ptr null, i32 1
  %t1 = ptrtoint ptr %t0 to i64
  %t2 = call ptr @malloc(i64 %t1)
  %t3 = getelementptr inbounds %Point, ptr %t2, i32 0, i32 0
  store double 0, ptr %t3
  %t4 = getelementptr inbounds %Point, ptr %t2, i32 0, i32 1
  store double 0, ptr %t4
  ret ptr %t2
}

define double @Point_getX (ptr %self) {
entry:
  %t5 = getelementptr inbounds %Point, ptr %self, i32 0, i32 0
  %t6 = load double, ptr %t5
  ret double %t6
}

define double @Point_getY (ptr %self) {
entry:
  %t7 = getelementptr inbounds %Point, ptr %self, i32 0, i32 1
  %t8 = load double, ptr %t7
  ret double %t8
}

define double @Point_setX (ptr %self, double %param_x) {
entry:
  %t9 = alloca double
  store double %param_x, ptr %t9
  %t10 = load double, ptr %t9
  ; GEP para campo 'x' sobre instancia %self
  %t11 = getelementptr inbounds %Point, ptr %self, i32 0, i32 0 ; campo x
  store double %t10, ptr %t11
  ret double %t10
}

define double @Point_setY (ptr %self, double %param_y) {
entry:
  %t12 = alloca double
  store double %param_y, ptr %t12
  %t13 = load double, ptr %t12
  ; GEP para campo 'y' sobre instancia %self
  %t14 = getelementptr inbounds %Point, ptr %self, i32 0, i32 1 ; campo y
  store double %t13, ptr %t14
  ret double %t13
}

define i32 @main() {
entry:
  %t15 = call ptr @Point_new()
  %t16 = alloca ptr
  store ptr %t15, ptr %t16
  %t17 = load ptr, ptr %t16
  %t18 = call double @Unknown_getX(ptr %t17)
  %t19 = load ptr, ptr %t16
  %t20 = call double @Unknown_getY(ptr %t19)
  %t21 = fadd double %t18, %t20
  ret i32 0
}