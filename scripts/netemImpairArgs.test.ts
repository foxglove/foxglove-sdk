import { resolveArgs } from "./netemImpairArgs";

describe("resolveArgs", () => {
  it("resolves a known profile to its netem args", () => {
    expect(resolveArgs({ profile: "pristine" }, [])).toEqual(["delay", "0ms"]);
    expect(resolveArgs({ profile: "severe" }, [])).toEqual([
      "delay",
      "100ms",
      "30ms",
      "loss",
      "5%",
      "rate",
      "2mbit",
    ]);
  });

  it("passes raw trailing args through unchanged", () => {
    expect(resolveArgs({}, ["delay", "5ms", "loss", "1%"])).toEqual(["delay", "5ms", "loss", "1%"]);
  });

  it("rejects an unknown profile", () => {
    expect(() => resolveArgs({ profile: "bogus" }, [])).toThrow(/unknown profile 'bogus'/);
  });

  it("rejects combining a profile with raw args", () => {
    expect(() => resolveArgs({ profile: "severe" }, ["delay", "5ms"])).toThrow(/either/);
  });

  it("rejects when neither a profile nor args are given", () => {
    expect(() => resolveArgs({}, [])).toThrow(/nothing to apply/);
  });
});
