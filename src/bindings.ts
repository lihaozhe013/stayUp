// Generated from the Rust domain models. Re-run `bun run types:generate` after editing them.

export type AppError = { code: string, message: string, details: string | null, };

export type EnvironmentVariable = { name: string, value: string, };

export type ProcessConfig = { executable: string, arguments: string, workingDirectory: string, environment: Array<EnvironmentVariable>, };

export type RestartPolicy = "never" | "onFailure";

export type RestartConfig = { policy: RestartPolicy, delaySeconds: number, };

export type AdvancedConfig = { stopTimeoutSeconds: number, };

export type AppDraft = { name: string, process: ProcessConfig, startupEnabled: boolean, restart: RestartConfig, loggingEnabled: boolean, advanced: AdvancedConfig, };

export type ManagedApp = { id: string, revision: number, name: string, ownerSid: string, process: ProcessConfig, startupEnabled: boolean, restart: RestartConfig, loggingEnabled: boolean, advanced: AdvancedConfig, createdAt: string, updatedAt: string, schemaVersion: number, backend: string, winswVersion: string, };

export type AppTemplate = "backgroundApplication" | "localServer" | "backgroundWorker" | "custom";

export type AppAction = "start" | "stop" | "restart";

export type ServiceStatus = "running" | "startPending" | "stopPending" | "stopped" | "missing" | "unknown" | "needsAttention";

export type AppOverview = { app: ManagedApp, status: ServiceStatus, statusDetail: string | null, };

export type LogStream = "stdout" | "stderr" | "diagnostic";

export type LogResponse = { content: string, truncated: boolean, byteLimit: number, };
