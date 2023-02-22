import { ensureDir } from "https://deno.land/std@0.175.0/fs/ensure_dir.ts";
import { join } from "https://deno.land/std@0.175.0/path/mod.ts";
import init, {
  parcelCSS,
  parseDeps as parseDepsWasmFn,
  transform as transformWasmFn,
} from "./dist/compiler.js";
import wasm from "./dist/wasm.js";
import type {
  DependencyDescriptor,
  TransformCSSOptions,
  TransformCSSResult,
  TransformOptions,
  TransformResult,
} from "./types.ts";
import { VERSION } from "./version.ts";

let modulesCache: string | null = null;
let wasmReady: Promise<void> | boolean = false;

if (typeof Deno.run === "function") {
  const p = Deno.run({
    cmd: [Deno.execPath(), "info", "--json"],
    stdout: "piped",
    stderr: "null",
  });
  const output = (new TextDecoder()).decode(await p.output());
  const info = JSON.parse(output);
  modulesCache = info?.modulesCache || null;
  await p.status();
  p.close();
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

/** initialize the compiler wasm module. */
export async function initWasm() {
  if (import.meta.url.startsWith("https://") && modulesCache) {
    const cacheDir = join(
      modulesCache,
      `https/deno.land/x/aleph_compiler@${VERSION}/dist`,
    );
    const cachePath = join(cacheDir, "compiler.wasm");
    if (await existsFile(cachePath)) {
      const file = await Deno.open(cachePath, { read: true });
      await init(
        new Response(file.readable, {
          headers: [["Content-Type", "application/wasm"]],
        }),
      );
    } else {
      const wasmData = await wasm();
      await init(wasmData);
      await ensureDir(cacheDir);
      await Deno.writeFile(cachePath, new Uint8Array(wasmData));
    }
  } else {
    await init(await wasm());
  }
  wasmReady = true;
}

async function getWasmReady() {
  if (wasmReady === true) return;
  if (wasmReady === false) {
    wasmReady = initWasm().catch(() => {
      wasmReady = false;
    });
  }
  await wasmReady;
}

/** Parse the deps of the modules. */
export async function parseDeps(
  specifier: string,
  code: string,
  options: Pick<TransformOptions, "importMap" | "lang"> = {},
): Promise<DependencyDescriptor[]> {
  await getWasmReady();
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
 *    export default function App() {
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
  await getWasmReady();
  try {
    return transformWasmFn(specifier, code, options);
  } catch (error) {
    if (
      options.minify &&
      (error.stack ?? error.messsage ?? "").includes("ThreadPoolBuildError")
    ) {
      // retry and disable minify if ThreadPoolBuildError
      if (options.minify.compress) {
        return await transform(specifier, code, {
          ...options,
          minify: { compress: false },
        });
      } else {
        return transformWasmFn(specifier, code, {
          ...options,
          minify: undefined,
        });
      }
    } else {
      throw error;
    }
  }
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
  await getWasmReady();
  return parcelCSS(specifier, code, options);
}
