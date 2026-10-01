import { afterEach, describe, expect, it, vi } from "vitest";

import { getManagedApp, isDesktopApp, listManagedApps, validateAppDraft } from "./api";

const invoke = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("desktop API boundary", () => {
  afterEach(() => {
    invoke.mockReset();
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("does not call native commands from a regular browser", async () => {
    expect(isDesktopApp()).toBe(false);
    await expect(listManagedApps()).rejects.toThrow("Open StayUp as a Windows desktop app");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("routes desktop calls through the typed Tauri command name", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    invoke.mockResolvedValue([]);

    await expect(listManagedApps()).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledWith("list_managed_apps");
  });

  it("validates a draft through the Rust domain boundary", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    invoke.mockResolvedValue(undefined);
    const draft = {
      name: "Local API",
      process: { executable: "C:\\Tools\\api.exe", arguments: "", workingDirectory: "C:\\Tools", environment: [] },
      startupEnabled: true,
      restart: { policy: "onFailure" as const, delaySeconds: 5 },
      loggingEnabled: true,
      advanced: { stopTimeoutSeconds: 15 },
    };

    await expect(validateAppDraft(draft)).resolves.toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("validate_app_draft", { draft });
  });

  it("loads a managed app by its stable id", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", { configurable: true, value: {} });
    invoke.mockResolvedValue({ id: "app-id" });

    await expect(getManagedApp("app-id")).resolves.toEqual({ id: "app-id" });
    expect(invoke).toHaveBeenCalledWith("get_managed_app", { appId: "app-id" });
  });
});
