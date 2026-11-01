import { validatePin, validatePort } from "./validate";

describe("validatePort", () => {
  it("accepts 1024..65535", () => {
    expect(validatePort("1024")).toBeNull();
    expect(validatePort("65535")).toBeNull();
    expect(validatePort("7331")).toBeNull();
  });
  it("rejects everything else", () => {
    expect(validatePort("80")).toMatch(/1024/);
    expect(validatePort("65536")).toMatch(/65535/);
    expect(validatePort("abc")).toMatch(/number/);
    expect(validatePort("")).toMatch(/number/);
  });
});

describe("validatePin", () => {
  it("accepts 4 to 8 digits", () => {
    expect(validatePin("1234")).toBeNull();
    expect(validatePin("12345678")).toBeNull();
  });
  it("rejects wrong length or non digits", () => {
    expect(validatePin("123")).toMatch(/4 to 8 digits/);
    expect(validatePin("123456789")).toMatch(/4 to 8 digits/);
    expect(validatePin("12a4")).toMatch(/4 to 8 digits/);
  });
});
