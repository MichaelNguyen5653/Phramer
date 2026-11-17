import { describe, expect, it } from "vitest";
import { FORMATS, FORMATS_HINT, formatLabel, formatOf } from "./formats";

describe("the formats Phramer saves", () => {
  it("are five, named for the user and by their extension", () => {
    expect(FORMATS.map((format) => [format.id, format.label])).toEqual([
      ["png", "PNG"],
      ["jpg", "JPEG"],
      ["bmp", "BMP"],
      ["webp", "WebP"],
      ["pdf", "PDF"],
    ]);
    expect(FORMATS_HINT).toBe("Phramer saves PNG, JPEG, BMP, WebP or PDF");
  });

  it("are found by extension in any case, with or without the dot; jpeg is JPEG", () => {
    expect(formatOf("png")).toBe("png");
    expect(formatOf(".PDF")).toBe("pdf");
    expect(formatOf("WebP")).toBe("webp");
    expect(formatOf("jpeg")).toBe("jpg");
    expect(formatOf("JPG")).toBe("jpg");
    for (const other of ["tif", "tiff", "gif", "ico", "", "png2", "2"]) expect(formatOf(other)).toBeNull();
  });

  it("label a stored extension, and say another as it is", () => {
    expect(formatLabel("jpg")).toBe("JPEG");
    expect(formatLabel("jpeg")).toBe("JPEG");
    expect(formatLabel("webp")).toBe("WebP");
    expect(formatLabel("tif")).toBe("TIF");
  });
});
