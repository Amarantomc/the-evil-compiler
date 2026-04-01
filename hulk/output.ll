; ModuleID = 'hulk_module'
source_filename = "hulk_module"

define i32 @main() {
entry:
  %num7 = alloca i32, align 4
  %num5 = alloca i32, align 4
  %num3 = alloca i32, align 4
  %num1 = alloca i32, align 4
  %num = alloca i32, align 4
  store i32 2, ptr %num, align 4
  %num_val = load i32, ptr %num, align 4
  store i32 2, ptr %num1, align 4
  %num_val2 = load i32, ptr %num1, align 4
  %add = add i32 %num_val, %num_val2
  store i32 4, ptr %num3, align 4
  %num_val4 = load i32, ptr %num3, align 4
  %mul = mul i32 %add, %num_val4
  store i32 5, ptr %num5, align 4
  %num_val6 = load i32, ptr %num5, align 4
  store i32 5, ptr %num7, align 4
  %num_val8 = load i32, ptr %num7, align 4
  %add9 = add i32 %num_val6, %num_val8
  %gt = icmp sgt i32 %mul, %add9
  %gt_ext = zext i1 %gt to i32
  ret i32 %gt_ext
}
