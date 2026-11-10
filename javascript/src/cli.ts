#!/usr/bin/env node
import { native } from "./native.js";

const ARGUMENT_START = 2;
const result = native.cli(process.argv.slice(ARGUMENT_START));
process.stdout.write(result.stdout);
process.stderr.write(result.stderr);
process.exitCode = result.exitCode;
