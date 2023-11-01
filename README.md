to build run 
```
RUSTFLAGS="-Zlocation-detail=none" cargo +nightly build -Z build-std=std,panic_abort --target aarch64-unknown-linux-gnu --release
```
and then
```
upx --best --lzma target/aarch64-unknown-linux-gnu/release/flash-module
```
Make sure it is compiled using glibc 2.31 or lower