[![Aleph.js: The Full-stack Framework in Deno.](https://raw.githubusercontent.com/alephjs/aleph-compiler/main/.github/poster.svg)](https://alephjs.org)

# Aleph.js Compiler

The compiler of Aleph.js written in Rust, powered by [swc](https://swc.rs) and [lightningcss](https://lightningcss.dev/).

## Usage

```ts
import { transform } from "https://deno.land/x/aleph_compiler@0.8.4/mod.ts";

const code = `
import { useState, useEffect } from "react"

export default function App() {
  const [msg, setMsg] = useState("...")

  useEffect(() => {
    setTimeout(() => {
      setMsg("world!")
    }, 1000)
  }, [])

  return <h1>Hello {msg}</h1>
}
`

const ret = await transform("./app.tsx", code, {
  importMap: JSON.stringify({
    imports: {
      "react": "https://esm.sh/react@18",
    }
  }),
  jsxImportSource: "https://esm.sh/react@18",
  sourceMap: true,
})

console.log(ret.code, ret.map)
```

## Development Setup

You will need [rust](https://www.rust-lang.org/tools/install) 1.60+ and [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/).

## Build

```bash
deno run -A build.ts
```

## Run tests

```bash
cargo test --all
```
