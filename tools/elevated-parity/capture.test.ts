import { expect, test } from "bun:test";
import { timing } from "./capture.ts";

const w = (lag: number, corr: number) => ({ start: 0, lag, corr });

test("weak windows at random lags do not fail a consistent song", () => {
  const t = timing([w(-6208, 0.31), w(3392, 0.43), w(3328, 0.71), w(3328, 0.82), w(3392, 0.78), w(3328, 0.82), w(3392, 0.65)]);
  expect(t).toMatchObject({ confident: 5, windows: 7, lag: 3328, outliers: 0, pass: true });
});

test("a confident window off the common lag fails", () => {
  expect(timing([w(0, 0.8), w(64, 0.9), w(2000, 0.7)]).pass).toBe(false);
});

test("fewer than 60 % confident windows fail", () => {
  expect(timing([w(0, 0.8), w(0, 0.4), w(0, 0.3)]).pass).toBe(false);
});

test("no windows fail", () => {
  expect(timing([]).pass).toBe(false);
});
