import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { readAsset } from "./readme-assets";

const LIMIT = 64 * 1024 * 1024;
let directory: string;
let file: string;

beforeEach(async () => {
  directory = await fs.mkdtemp(path.join(os.tmpdir(), "readme-reader-"));
  file = path.join(directory, "image.png");
  await fs.writeFile(file, "");
});

afterEach(async () => {
  vi.restoreAllMocks();
  await fs.rm(directory, { recursive: true, force: true });
});

async function trackHandle() {
  const handle = await fs.open(file, "r");
  vi.spyOn(fs, "open").mockResolvedValueOnce(handle);
  return {
    handle,
    read: vi.spyOn(handle, "read"),
    close: vi.spyOn(handle, "close"),
  };
}

describe("local README image source reader", () => {
  it.each([LIMIT - 1, LIMIT])("accepts a %i byte source", async (size) => {
    await fs.truncate(file, size);
    const { close } = await trackHandle();
    const bytes = await readAsset(directory, "image.png", "image.png");
    expect(bytes.length).toBe(size);
    expect(bytes[0]).toBe(0);
    expect(bytes[size - 1]).toBe(0);
    expect(close).toHaveBeenCalledOnce();
  });

  it("rejects an oversized source before reading content and closes the handle", async () => {
    await fs.truncate(file, LIMIT + 1);
    const { read, close } = await trackHandle();
    await expect(readAsset(directory, "image.png", "./image.png")).rejects.toThrow(
      "README image ./image.png exceeds the 64 MiB (67108864 byte) source limit",
    );
    expect(read).not.toHaveBeenCalled();
    expect(close).toHaveBeenCalledOnce();
  });

  it("bounds reads to limit plus one when the opened file outgrows its reported size", async () => {
    await fs.truncate(file, LIMIT + 100);
    const { handle, read, close } = await trackHandle();
    const stats = await handle.stat();
    stats.size = 0;
    vi.spyOn(handle, "stat").mockResolvedValueOnce(stats);
    await expect(readAsset(directory, "image.png", "image.png")).rejects.toThrow("64 MiB");
    const lengths = read.mock.calls.map(([options]) => options?.length ?? 0);
    expect(lengths.every((length) => length > 0 && length <= 64 * 1024)).toBe(true);
    expect(lengths.reduce((total, length) => total + length, 0)).toBe(LIMIT + 1);
    expect(lengths.at(-1)).toBe(1);
    expect(close).toHaveBeenCalledOnce();
  });

  it("returns all bytes across short reads and closes the handle", async () => {
    await fs.writeFile(file, "image bytes");
    const { handle, read, close } = await trackHandle();
    const originalRead = handle.read.bind(handle);
    read.mockImplementationOnce(async (options) => originalRead({ ...options, length: 2 }));
    expect((await readAsset(directory, "image.png", "image.png")).toString()).toBe("image bytes");
    expect(close).toHaveBeenCalledOnce();
  });

  it("accepts an empty file and closes the handle", async () => {
    const { close } = await trackHandle();
    expect(await readAsset(directory, "image.png", "image.png")).toEqual(Buffer.alloc(0));
    expect(close).toHaveBeenCalledOnce();
  });

  it.each(["stat", "read"] as const)("closes the handle when %s fails", async (operation) => {
    const { handle, close } = await trackHandle();
    vi.spyOn(handle, operation).mockRejectedValueOnce(new Error("I/O failed"));
    await expect(readAsset(directory, "image.png", "image.png")).rejects.toThrow("I/O failed");
    expect(close).toHaveBeenCalledOnce();
  });
});
