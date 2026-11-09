import { copyFileSync, mkdirSync } from "node:fs";
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
