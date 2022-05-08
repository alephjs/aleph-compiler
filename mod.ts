import { ensureDir } from "https://deno.land/std@0.136.0/fs/ensure_dir.ts";
import { dirname, join } from "https://deno.land/std@0.136.0/path/mod.ts";
import { VERSION } from "./version.ts";
import init, {
  parseDeps as parseDepsWasmFn,
  parseExportNames as parseExportNamesWasmFn,
  transform as transformWasmFn,
  transformCSS as parcelCSS,
} from "./dist/compiler.js";
import decodeWasmData from "./dist/wasm.js";
import {
  DependencyDescriptor,
  TransformCSSOptions,
  TransformCSSResult,
  TransformOptions,
  TransformResult,
} from "./types.ts";

let wasmReady: Promise<void> | boolean = false;

async function checkWasmReady() {
  if (wasmReady === false) {
    wasmReady = initWasm();
  }
  if (wasmReady instanceof Promise) {
    await wasmReady;
    wasmReady = true;
  }
}

/* check whether or not the given path exists as regular file. */
async function existsFile(path: string): Promise<boolean> {
  try {
    const stat = await Deno.lstat(path);
    return stat.isFile;
  } catch (err) {
    if (err instanceof Deno.errors.NotFound) {
      return false;
    }
    throw err;
  }
}

/* initialize the compiler wasm module. */
async function initWasm() {
  const mcDir = Deno.env.get("MODULES_CACHE_DIR");
  if (mcDir) {
    const cacheDir = join(mcDir, `https/deno.land/x/aleph_compiler/dist`);
    const cachePath = `${cacheDir}/aleph_compiler.${VERSION}.wasm`;
    if (await existsFile(cachePath)) {
      const wasmData = await Deno.readFile(cachePath);
      await init(wasmData);
    } else {
      const wasmData = decodeWasmData();
      await init(wasmData);
      await ensureDir(dirname(cachePath));
      await Deno.writeFile(cachePath, wasmData);
    }
  } else {
    const wasmData = decodeWasmData();
    await init(wasmData);
  }
}

/** Parse export names of the module. */
export async function parseExportNames(
  specifier: string,
  code: string,
): Promise<string[]> {
  await checkWasmReady();
  return parseExportNamesWasmFn(specifier, code);
}

/** Parse the deps of the modules. */
export async function parseDeps(
  specifier: string,
  code: string,
  options: Pick<TransformOptions, "importMap" | "lang"> = {},
): Promise<DependencyDescriptor[]> {
  await checkWasmReady();
  return parseDepsWasmFn(specifier, code, options);
}

/**
 * Transforms the JSX/TS module into a JS module.
 *
 * ```tsx
 * transform(
 *   '/app.tsx',
 *   `
 *    import React from 'https://esm.sh/react';
 *
 *    export default App() {
 *      return <h1>Hello world!</h1>
 *    }
 *   `
 * )
 * ```
 */
export async function transform(
  specifier: string,
  code: string,
  options: TransformOptions = {},
): Promise<TransformResult> {
  await checkWasmReady();
  return transformWasmFn(specifier, code, options);
}

/**
 * Compiles a CSS file, including optionally minifying and lowering syntax to the given
 * targets. A source map may also be generated, but this is not enabled by default.
 */
export async function transformCSS(
  specifier: string,
  code: string,
  options: TransformCSSOptions = {},
): Promise<TransformCSSResult> {
  await checkWasmReady();
  return parcelCSS(specifier, code, options);
}
