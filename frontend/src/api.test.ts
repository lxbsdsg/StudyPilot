import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./capabilities", () => ({
  isTauriRuntime: () => false,
}));

import { requestHttp } from "./api";

describe("HTTP bridge", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("returns undefined for 204 responses", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 204 })));
    await expect(requestHttp<void>("/api/reminders/one", { method: "DELETE" })).resolves.toBe(
      undefined,
    );
  });

  it("returns undefined when a successful legacy delete returns an empty JSON body", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response("null", {
          status: 200,
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );
    await expect(requestHttp<void>("/api/reminders/one")).resolves.toBe(undefined);
  });

  it("surfaces server detail messages", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ detail: "提醒时间格式无效" }), {
          status: 400,
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );
    await expect(requestHttp("/api/reminders")).rejects.toThrow("提醒时间格式无效");
  });
});
