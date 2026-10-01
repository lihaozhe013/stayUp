import { invoke } from "@tauri-apps/api/core";
import type {
  AppOverview,
  AppTemplate,
  LogResponse,
  LogStream,
  ManagedApp,
  ProcessConfig,
  RestartConfig,
  AdvancedConfig,
} from "./bindings";

export interface AppDraft {
  name: string;
  process: ProcessConfig;
  startupEnabled: boolean;
  restart: RestartConfig;
  loggingEnabled: boolean;
  advanced: AdvancedConfig;
}

export type TemplateDefaults = Omit<AppDraft, "name" | "process">;

export const templateDefaults: Record<AppTemplate, TemplateDefaults> = {
  backgroundApplication: {
    startupEnabled: true,
    restart: { policy: "onFailure", delaySeconds: 5 },
    loggingEnabled: true,
    advanced: { stopTimeoutSeconds: 15 },
  },
  localServer: {
    startupEnabled: true,
    restart: { policy: "onFailure", delaySeconds: 5 },
    loggingEnabled: true,
    advanced: { stopTimeoutSeconds: 15 },
  },
  backgroundWorker: {
    startupEnabled: true,
    restart: { policy: "onFailure", delaySeconds: 5 },
    loggingEnabled: true,
    advanced: { stopTimeoutSeconds: 15 },
  },
  custom: {
    startupEnabled: false,
    restart: { policy: "never", delaySeconds: 5 },
    loggingEnabled: true,
    advanced: { stopTimeoutSeconds: 15 },
  },
};

export const isDesktopApp = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const requireDesktopApp = (): void => {
  if (!isDesktopApp()) {
    throw new Error("Open StayUp as a Windows desktop app to manage background apps.");
  }
};

export async function listManagedApps(): Promise<AppOverview[]> {
  requireDesktopApp();
  return invoke("list_managed_apps");
}

export async function getManagedApp(appId: string): Promise<ManagedApp> {
  requireDesktopApp();
  return invoke("get_managed_app", { appId });
}

export async function validateAppDraft(draft: AppDraft): Promise<void> {
  requireDesktopApp();
  await invoke("validate_app_draft", { draft });
}

export async function getCurrentOwnerSid(): Promise<string> {
  requireDesktopApp();
  return invoke("get_current_owner_sid");
}

export async function createManagedApp(draft: AppDraft, startNow: boolean): Promise<AppOverview> {
  requireDesktopApp();
  return invoke("create_managed_app", { draft, startNow });
}

export async function updateManagedApp(
  appId: string,
  expectedRevision: number,
  draft: AppDraft,
): Promise<AppOverview> {
  requireDesktopApp();
  return invoke("update_managed_app", { appId, expectedRevision, draft });
}

export async function performAppAction(
  action: "start" | "stop" | "restart",
  appId: string,
): Promise<AppOverview> {
  requireDesktopApp();
  const command = { start: "start_app", stop: "stop_app", restart: "restart_app" }[action];
  await invoke(command, { appId });
  const apps = await listManagedApps();
  const overview = apps.find(({ app }) => app.id === appId);
  if (!overview) throw new Error("The managed app could not be found after the operation.");
  return overview;
}

export async function setStartup(appId: string, enabled: boolean): Promise<AppOverview> {
  requireDesktopApp();
  await invoke("set_startup", { appId, enabled });
  const apps = await listManagedApps();
  const overview = apps.find(({ app }) => app.id === appId);
  if (!overview) throw new Error("The managed app could not be found after the operation.");
  return overview;
}

export async function removeManagedApp(appId: string, removeLogs: boolean): Promise<void> {
  requireDesktopApp();
  await invoke("remove_managed_app", { appId, removeLogs });
}

export async function readAppLogs(
  appId: string,
  stream: LogStream,
): Promise<LogResponse> {
  requireDesktopApp();
  return invoke("read_app_logs", { appId, stream });
}

export async function openAppFolder(appId: string): Promise<void> {
  requireDesktopApp();
  await invoke("open_executable_directory", { appId });
}

export async function openLogFolder(appId: string): Promise<void> {
  requireDesktopApp();
  await invoke("open_log_directory", { appId });
}
