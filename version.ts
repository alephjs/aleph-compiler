/** `VERSION` managed by https://deno.land/x/publish */
export const VERSION = "0.7.4";

/** `prepublish` will be invoked before publish */
export async function prepublish(version: string): Promise<boolean> {
  const p = Deno.run({
    cmd: ["deno", "run", "-A", "build.ts"],
    stdout: "inherit",
    stderr: "inherit",
  });
  const { success } = await p.status();
  if (success) {
    const toml = await Deno.readTextFile("./Cargo.toml");
    await Deno.writeTextFile(
      "./Cargo.toml",
      toml.replace(/version = "[\d\.]+"/, `version = "${version}"`),
    );
  }
  p.close();
  return success;
}
