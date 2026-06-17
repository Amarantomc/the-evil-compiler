build:
	cargo build --release --manifest-path compiler/Cargo.toml
	cp compiler/target/release/hulk ./hulk

clean:
	cargo clean --manifest-path compiler/Cargo.toml
	rm -f ./hulk ./output ./output.ll ./output.o