set -e

# Run the Rust project to generate output.ll
cargo run

# Compile the LLVM IR into an executable
clang output.ll -isysroot $(xcrun --show-sdk-path) -o hulk_executable

# Run the resulting executable
./hulk_executable