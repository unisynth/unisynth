// Vite config of the DAW page (change daw-solid-ts): compiles the SolidJS UI.
// build: only the page entry `src/main.ts` → `dist/main.js` (fixed name, no hash; index.html, style.css, the
//   worklet, the bounce workers and the engine modules stay with build.ts, so the CLI server, build-static.ts and
//   the token injection see the same dist/ as before).
// dev (`pnpm --filter @64k/daw dev`, needs `bun packages/cli/src/cli.ts serve --no-headless` on DAW_API):
//   serves src/ at /daw/ with hot reload of components; /api and the engine files come from the CLI server.
import { defineConfig, type Plugin } from "vite";
import solid from "vite-plugin-solid";
import { fileURLToPath } from "node:url";

const SRC = fileURLToPath(new URL("./src", import.meta.url));
const API = process.env.DAW_API ?? "http://127.0.0.1:6464";

/** Dev only: index.html loads the built `main.js`; point it at the TS entry. */
const devEntry: Plugin = {
  name: "daw-dev-entry",
  apply: "serve",
  transformIndexHtml: { order: "pre", handler: (html) => html.replace('src="main.js"', 'src="./main.ts"') },
};

export default defineConfig(({ command }) => ({
  root: SRC,
  // `vite build` must pick solid-js's production build whatever NODE_ENV says: unset (Bun scripts) or "test" (bun test)
  // left Vite on the development condition, and the page shipped Solid's dev runtime (devComponent, dev checks).
  ...(command === "build" ? { mode: "production", resolve: { conditions: ["module", "browser", "production"] }, define: { "process.env.NODE_ENV": JSON.stringify("production") } } : {}),
  base: "/daw/",
  plugins: [solid(), devEntry],
  server: {
    host: "127.0.0.1",
    port: 6465,
    strictPort: true,
    // the threaded bounce needs cross-origin isolation (same headers as packages/cli/src/server.ts)
    headers: {
      "cross-origin-opener-policy": "same-origin",
      "cross-origin-embedder-policy": "require-corp",
      "cross-origin-resource-policy": "same-origin",
    },
    proxy: {
      "/api": API,
      "^/daw/(synth(-mt)?\\.wasm|worklet\\.js|bounce-worker\\.js|bounce-helper\\.js)$": API,
    },
  },
  build: {
    outDir: "../dist",
    emptyOutDir: false,
    sourcemap: "inline",
    minify: false,
    target: "es2022",
    rollupOptions: {
      input: fileURLToPath(new URL("./src/main.ts", import.meta.url)),
      output: { entryFileNames: "main.js", chunkFileNames: "[name].js", format: "es" },
    },
  },
  logLevel: "warn",
}));
