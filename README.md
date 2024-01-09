to build run 
```
RUSTFLAGS="-Zlocation-detail=none" cargo +nightly build -Z build-std=std,panic_abort --target aarch64-unknown-linux-gnu --release
```
if you do not have the right glibc version you can install [zig](https://github.com/ziglang/zig/wiki/Install-Zig-from-a-Package-Manager) and [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) and the run
```
RUSTFLAGS="-Zlocation-detail=none -C target-cpu=cortex-a53" cargo +nightly zigbuild -Z build-std=std,panic_abort --target aarch64-unknown-linux-gnu.2.31 --release
```
replace `.2.31` with the version you require, 2.31 is the version for debian 11 bullseye
and then
```
upx --best --lzma target/aarch64-unknown-linux-gnu/release/go-modules
```
to compress it