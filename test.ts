import {
  assertEquals,
  assertStringIncludes,
} from "https://deno.land/std@0.155.0/testing/asserts.ts";
import { transform, transformCSS } from "./mod.ts";

Deno.test("aleph compiler", async (t) => {
  await t.step("transform css", async () => {
    const ret = await transformCSS(
      "./app.css",
      `@custom-media --modern (color), (hover);

      .foo {
        background: yellow;

        -webkit-border-radius: 2px;
        -moz-border-radius: 2px;
        border-radius: 2px;

        -webkit-transition: background 200ms;
        -moz-transition: background 200ms;
        transition: background 200ms;

        &.bar {
          color: green;
        }
      }

      @media (--modern) and (width > 1024px) {
        .a {
          color: green;
        }
      }`,
      {
        minify: true,
        targets: {
          chrome: 95,
        },
        drafts: {
          nesting: true,
          customMedia: true,
        },
      },
    );

    assertEquals(
      ret.code,
      `.foo{background:#ff0;border-radius:2px;transition:background .2s}.foo.bar{color:green}@media ((color) or (hover)) and (min-width:1024px){.a{color:green}}`,
    );
  });

  await t.step("transform ts", async () => {
    const ret = await transform(
      "./mod.ts",
      await Deno.readTextFile("./mod.ts"),
    );

    assertStringIncludes(ret.code, `function transform(`);
  });

  await t.step("transform jsx", async () => {
    const ret = await transform(
      "./app.jsx",
      `
        import React from "https://esm.sh/react";

        export default function App() {
          return <h1>Hello world!</h1>
        }
      `,
    );

    assertStringIncludes(ret.code, `React.createElement("h1"`);
  });

  await t.step("transform jsx (jsxImportSource)", async () => {
    const ret = await transform(
      "./app.jsx",
      `
        export default function App() {
          return <h1>Hello world!</h1>
        }
      `,
      {
        jsxImportSource: "https://esm.sh/react",
      },
    );
    assertStringIncludes(
      ret.code,
      `import { jsx as _jsx } from "/-/esm.sh/react/jsx-runtime"`,
    );
    assertStringIncludes(ret.code, `_jsx("h1"`);
  });

  await t.step("transform large js", async () => {
    const ret = await transform(
      "./gsi-client.js",
      await Deno.readTextFile("./testdata/gsi-client.js"),
      { minify: { compress: true } },
    );

    assertStringIncludes(ret.code, `this.default_gsi`);
  });
});
