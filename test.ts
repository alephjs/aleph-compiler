import "https://deno.land/x/global@0.144.0/testing.ts";
import { transform } from "./mod.ts";

Deno.test("swc", async (t) => {
  await t.step("ts", async () => {
    const ret = await transform(
      "./mod.ts",
      await Deno.readTextFile("./mod.ts"),
    );

    assert(ret.code.includes(`function transform(`));
  });

  await t.step("jsx", async () => {
    const ret = await transform(
      "./app.jsx",
      `
        import React from "https://esm.sh/react";

        export default function App() {
          return <h1>Hello world!</h1>
        }
      `,
    );

    assert(ret.code.includes(`React.createElement("h1"`));
  });

  await t.step("js", async () => {
    const ret = await transform(
      "./gsi-client.js",
      await Deno.readTextFile("./testdata/gsi-client.js"),
      { minify: { compress: true } },
    );

    assert(ret.code.includes(`this.default_gsi`));
  });
});
