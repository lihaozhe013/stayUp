import {
  Ellipsis,
  FileOutput,
  FolderOpen,
  Pencil,
  Play,
  RefreshCw,
  ShieldCheck,
  Square,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import type { AppOverview, ManagedApp } from "../bindings";
import { openAppFolder, openLogFolder, setStartup } from "../api";

interface AppCardProps {
  overview: AppOverview;
  onEdit: (app: ManagedApp) => void;
  onLogs: (app: ManagedApp) => void;
  onAction: (action: "start" | "stop" | "restart", app: ManagedApp) => void;
  onRemove: (app: ManagedApp) => void;
  onUpdated: (overview: AppOverview) => void;
  onError: (message: string) => void;
}

const statusLabels = {
  running: "Running",
  startPending: "Starting",
  stopPending: "Stopping",
  stopped: "Stopped",
  missing: "Service missing",
  unknown: "Status unavailable",
  needsAttention: "Needs attention",
} as const;

const shortPath = (path: string, maxLength = 54): string => {
  if (path.length <= maxLength) return path;
  return `…${path.slice(-(maxLength - 1))}`;
};

export function AppCard({ overview, onEdit, onLogs, onAction, onRemove, onUpdated, onError }: AppCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [savingStartup, setSavingStartup] = useState(false);
  const { app, status } = overview;
  const running = status === "running" || status === "startPending";
  const busy = status === "startPending" || status === "stopPending";
  const issue = status === "missing" || status === "unknown" || status === "needsAttention";

  async function toggleStartup(enabled: boolean) {
    setSavingStartup(true);
    try {
      onUpdated(await setStartup(app.id, enabled));
    } catch (error) {
      onError(error instanceof Error ? error.message : "Could not update startup settings.");
    } finally {
      setSavingStartup(false);
    }
  }

  async function openFolder(action: () => Promise<void>) {
    setMenuOpen(false);
    try {
      await action();
    } catch (error) {
      onError(error instanceof Error ? error.message : "Could not open the folder.");
    }
  }

  return (
    <article className={`app-card ${issue ? "app-card-warning" : ""}`}>
      <div className="app-card-main">
        <div className={`app-card-icon ${running ? "is-running" : ""}`} aria-hidden="true">
          <span>{app.name.trim().slice(0, 1).toUpperCase()}</span>
          <i />
        </div>
        <div className="app-card-copy">
          <div className="app-card-title-line">
            <h3>{app.name}</h3>
            <span className={`status-pill status-${status}`}>
              <i aria-hidden="true" />
              {statusLabels[status]}
            </span>
          </div>
          <p className="app-path" title={app.process.executable}>{shortPath(app.process.executable)}</p>
          <div className="app-card-meta">
            {app.startupEnabled && (
              <span><ShieldCheck aria-hidden="true" size={13} /> Starts with Windows</span>
            )}
            {app.restart.policy === "onFailure" && (
              <span><RefreshCw aria-hidden="true" size={13} /> Restarts after a crash</span>
            )}
            {app.loggingEnabled && (
              <span><FileOutput aria-hidden="true" size={13} /> Output saved</span>
            )}
          </div>
          {overview.statusDetail && <p className="card-status-detail">{overview.statusDetail}</p>}
        </div>
        <div className="app-card-actions">
          <button
            className={`button button-small ${running ? "button-secondary" : "button-primary"}`}
            disabled={busy}
            onClick={() => onAction(running ? "stop" : "start", app)}
          >
            {running ? <Square aria-hidden="true" size={14} /> : <Play aria-hidden="true" size={14} />}
            {running ? "Stop" : "Start"}
          </button>
          <button
            className="icon-button"
            title="Restart app"
            aria-label={`Restart ${app.name}`}
            disabled={busy || !running}
            onClick={() => onAction("restart", app)}
          ><RefreshCw aria-hidden="true" size={16} /></button>
          <button className="icon-button" title="View logs" aria-label={`View ${app.name} logs`} onClick={() => onLogs(app)}>
            <FileOutput aria-hidden="true" size={16} />
          </button>
          <button
            className="icon-button menu-anchor"
            title="More actions"
            aria-label={`More actions for ${app.name}`}
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen(!menuOpen)}
          ><Ellipsis aria-hidden="true" size={18} /></button>
          {menuOpen && (
            <div className="action-menu" role="menu">
              <button role="menuitem" onClick={() => { setMenuOpen(false); onEdit(app); }}><Pencil size={14} /> Edit settings</button>
              <button role="menuitem" onClick={() => { setMenuOpen(false); onLogs(app); }}><FileOutput size={14} /> View logs</button>
              <button role="menuitem" onClick={() => void openFolder(() => openAppFolder(app.id))}><FolderOpen size={14} /> Open program folder</button>
              <button role="menuitem" onClick={() => void openFolder(() => openLogFolder(app.id))}><FolderOpen size={14} /> Open log folder</button>
              <label className="action-menu-toggle">
                <input type="checkbox" checked={app.startupEnabled} disabled={savingStartup} onChange={(event) => void toggleStartup(event.target.checked)} />
                Start with Windows
              </label>
              <button className="action-danger" role="menuitem" onClick={() => { setMenuOpen(false); onRemove(app); }}><Trash2 size={14} /> Remove app</button>
            </div>
          )}
        </div>
      </div>
    </article>
  );
}
