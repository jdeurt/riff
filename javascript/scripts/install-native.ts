import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const names: Readonly<Record<string, string>> = {
  linux: "libriff_neon.so",
  darwin: "libriff_neon.dylib",
  win32: "riff_neon.dll",
};
const name = names[process.platform];
if (name === undefined) {
  throw new Error(`Unsupported native build platform: ${process.platform}`);
}
mkdirSync("native", { recursive: true });
copyFileSync(path.resolve("../target/release", name), path.resolve("native/index.node"));

copyFileSync("src/native-addon.d.ts", "dist/src/native-addon.d.ts");
// Declaration references must resolve within the installed package.
const declarations = readFileSync("dist/src/binding.d.cts", "utf8");
writeFileSync(
  "dist/src/binding.d.cts",
  declarations.replace("../../src/native-addon.d.ts", "./native-addon.d.ts"),
);
