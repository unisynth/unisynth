// Transport state helpers (change daw-solid-ts 3.3): position and time text, as the old bar wrote them.
import { expect, test } from "bun:test";
import { ensureSig, fmtPos, fmtTime, sig, sigOptions } from "./transport.ts";

test("position text: bars.beats.ticks in the signature's unit", () => {
  expect(fmtPos(1, "4/4")).toBe("1.1.0");
  expect(fmtPos(3.25, "4/4")).toBe("3.2.0");
  expect(fmtPos(3.375, "4/4")).toBe("3.2.480");
  expect(fmtPos(2.5, "3/4")).toBe("2.2.480");
  expect(fmtPos(1.5, "7/8")).toBe("1.4.240");
  expect(fmtPos(0.5, "4/4")).toBe("1.1.0");
});

test("time text: m:ss.mmm from bars, tempo and signature", () => {
  expect(fmtTime(1, 128, "4/4")).toBe("0:00.000");
  expect(fmtTime(3, 120, "4/4")).toBe("0:04.000");
  expect(fmtTime(3, 240, "4/4")).toBe("0:02.000");
  expect(fmtTime(33, 120, "4/4")).toBe("1:04.000");
  expect(fmtTime(2, 120, "7/8")).toBe("0:01.750");
});

test("ensureSig adds an unknown signature once and selects it", () => {
  ensureSig("5/4"); ensureSig("5/4");
  expect(sig()).toBe("5/4");
  expect(sigOptions().filter((o) => o === "5/4").length).toBe(1);
});
