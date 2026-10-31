// bun test preload (root bunfig.toml): Bun resolves solid-js to its server build, where signals and stores do not
// react, and ignores `bunfig` conditions and runtime `onResolve` for bare imports. Serve the browser builds' source
// when the server files load, so DAW unit tests (src/ui/state/*.test.ts) run the real reactive runtime (change
// daw-solid-ts). Affects only solid-js; nothing else in the repo imports it.
import { plugin } from "bun";
plugin({
  name: "solid-browser",
  setup(b) {
    b.onLoad({ filter: /solid-js\/(dist|store\/dist|web\/dist)\/server\.js$/ }, async (a) => {
      const file = a.path.replace(/dist\/server\.js$/, (m) => (a.path.includes("/store/") ? "dist/store.js" : a.path.includes("/web/") ? "dist/web.js" : "dist/solid.js"));
      return { contents: await Bun.file(file).text(), loader: "js" };
    });
  },
});
