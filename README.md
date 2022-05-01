[![Aleph.js: The Full-stack Framework in Deno.](.github/poster.svg)](https://alephjs.org)

# Aleph.js Compiler

The compiler of Aleph.js written in Rust, powered by [swc](https://github.com/swc-project/swc) and [parcel css](https://github.com/parcel-bundler/parcel-css).

## Development Setup

You will need [rust](https://www.rust-lang.org/tools/install) 1.53+ and [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/).

## Build

```bash
deno run -A build.ts
```

## Run tests

```bash
cargo test --all
```
