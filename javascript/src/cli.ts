#!/usr/bin/env node
import { callNative, native } from "./native.js";

const ARGUMENT_START = 2;
const result = callNative(() => native.cli(JSON.stringify(process.argv.slice(ARGUMENT_START))));
process.stdout.write(result.stdout);
process.stderr.write(result.stderr);
process.exitCode = result.exitCode;
