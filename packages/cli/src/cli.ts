#!/usr/bin/env bun
// unisynth DAW dev server.
//
//   bun packages/cli/src/cli.ts serve [--port 6464] [--build]   → http://127.0.0.1:6464/daw/
import { DEFAULT_PORT, startServer } from "./server.ts";

const argv = process.argv.slice(2);
const flag = (n: string) => {
  const i = argv.indexOf(`--${n}`);
  return i < 0 ? undefined : argv[i + 1];
};
if (argv[0] !== "serve") {
  console.error("usage: bun packages/cli/src/cli.ts serve [--port 6464] [--build]");
  process.exit(argv[0] === undefined || argv[0] === "help" ? 0 : 1);
}
const s = await startServer({ port: Number(flag("port") ?? DEFAULT_PORT), build: argv.includes("--build") });
console.log(`DAW: ${s.url}daw/`);
