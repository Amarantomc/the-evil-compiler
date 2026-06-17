set -e

# Run the Rust project to generate output.ll
cargo run

# Compile the LLVM IR into an executable
#clang output.ll -isysroot $(xcrun --show-sdk-path) -o hulk_executable
llc -filetype=obj output.ll -o output.obj 
gcc output.obj -o output.exe  

# Run the resulting executable
./output.exe 