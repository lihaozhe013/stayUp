import {
  Activity,
  ArrowDownUp,
  ArrowUpRight,
  Check,
  ChevronDown,
  CircleHelp,
  Command,
  Filter,
  Plus,
  RefreshCw,
  Search,
  Server,
  ShieldCheck,
  Sparkles,
  X,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppOverview, ManagedApp } from "./bindings";
import {
  createManagedApp,
  isDesktopApp,
  listManagedApps,
  performAppAction,
  removeManagedApp,
  updateManagedApp,
} from "./api";
import type { AppDraft } from "./api";
import { AppCard } from "./components/AppCard";
import { AppForm } from "./components/AppForm";
import { DialogFrame } from "./components/DialogFrame";
import { LogDialog } from "./components/LogDialog";
import { Toast } from "./components/Toast";
import type { Notice } from "./components/Toast";

type FilterMode = "all" | "running" | "stopped" | "attention";

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return "Something went wrong. Try again, or view the app logs for more detail.";
}

function isRunning(overview: AppOverview): boolean {
  return overview.status === "running" || overview.status === "startPending";
}

function isAttention(overview: AppOverview): boolean {
  return overview.status === "needsAttention" || overview.status === "missing" || overview.status === "unknown";
}

export default function App() {
  const desktop = isDesktopApp();
  const [apps, setApps] = useState<AppOverview[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [filter, setFilter] = useState<FilterMode>("all");
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<ManagedApp | "new" | null>(null);
  const [logsFor, setLogsFor] = useState<ManagedApp | null>(null);
  const [removing, setRemoving] = useState<ManagedApp | null>(null);
  const [removeLogs, setRemoveLogs] = useState(false);
  const [notices, setNotices] = useState<Notice[]>([]);

  const notify = useCallback((message: string, tone: Notice["tone"] = "success") => {
    const id = Date.now() + Math.floor(Math.random() * 1000);
    setNotices((current) => [...current.slice(-2), { id, message, tone }]);
    window.setTimeout(() => setNotices((current) => current.filter((notice) => notice.id !== id)), 5000);
  }, []);

  const refresh = useCallback(async (quiet = false) => {
    if (!desktop) {
      setLoading(false);
      return;
    }
    quiet ? setRefreshing(true) : setLoading(true);
    try {
      const latest = await listManagedApps();
      setApps(latest);
    } catch (error) {
      if (!quiet) notify(errorMessage(error), "error");
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [desktop, notify]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh(true);
    }, 2000);
    const resume = () => {
      if (document.visibilityState === "visible") void refresh(true);
    };
    document.addEventListener("visibilitychange", resume);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", resume);
    };
  }, [refresh]);

  const runningCount = apps.filter(isRunning).length;
  const stoppedCount = apps.filter(({ status }) => status === "stopped").length;
  const needsAttention = apps.filter(isAttention).length;
  const visibleApps = useMemo(() => apps.filter((overview) => {
    const matchesFilter = filter === "all"
      || (filter === "running" && isRunning(overview))
      || (filter === "stopped" && !isRunning(overview) && !isAttention(overview))
      || (filter === "attention" && isAttention(overview));
    const text = `${overview.app.name} ${overview.app.process.executable}`.toLocaleLowerCase();
    return matchesFilter && text.includes(query.trim().toLocaleLowerCase());
  }), [apps, filter, query]);

  async function saveApp(draft: AppDraft, startNow: boolean) {
    try {
      if (editing === "new") {
        const created = await createManagedApp(draft, startNow);
        await refresh(true);
        setEditing(null);
        if (startNow && created.status !== "running") {
          notify(`${draft.name.trim()} was added, but did not stay running. Review its logs or try starting it again.`, "error");
        } else {
          notify(startNow ? `${draft.name.trim()} is set up and ready.` : `${draft.name.trim()} was added.`);
        }
        return;
      }
      if (editing) {
        const current = apps.find(({ app }) => app.id === editing.id);
        if (!current) throw new Error("This app has changed or was removed. Refresh your list and try again.");
        const wasRunning = isRunning(current);
        if (wasRunning && !window.confirm("Saving these settings will restart the app. Continue?")) return;
        const updated = await updateManagedApp(editing.id, editing.revision, draft);
        setEditing(null);
        await refresh(true);
        if (wasRunning && updated.status !== "running") {
          notify(`${draft.name.trim()} settings were saved, but the app did not restart. Review its logs or start it again.`, "error");
        } else {
          notify(`${draft.name.trim()} settings were saved.`);
        }
      }
    } catch (error) {
      notify(errorMessage(error), "error");
      throw error;
    }
  }

  async function runAction(action: "start" | "stop" | "restart", app: ManagedApp) {
    try {
      const overview = await performAppAction(action, app.id);
      setApps((current) => current.map((item) => item.app.id === app.id ? overview : item));
      const message = {
        start: `${app.name} was started.`,
        stop: `${app.name} was stopped.`,
        restart: `${app.name} was restarted.`,
      }[action];
      notify(message);
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  async function confirmRemove() {
    if (!removing) return;
    try {
      await removeManagedApp(removing.id, removeLogs);
      setApps((current) => current.filter(({ app }) => app.id !== removing.id));
      setRemoving(null);
      setRemoveLogs(false);
      notify(`${removing.name} was removed.`);
    } catch (error) {
      notify(errorMessage(error), "error");
    }
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a className="brand" href="#home" aria-label="StayUp home">
          <span className="brand-mark"><span /><span /><span /></span>
          <span className="brand-name">stay<span>up</span><small>DESKTOP</small></span>
        </a>

        <div className="sidebar-section-label">YOUR WORKSPACE</div>
        <button className="sidebar-link is-active">
          <Activity aria-hidden="true" size={16} /><span>Managed apps</span><span className="sidebar-count">{apps.length.toString().padStart(2, "0")}</span>
        </button>

        <div className="sidebar-note">
          <div className="sidebar-note-icon"><ShieldCheck aria-hidden="true" size={16} /></div>
          <p>Runs under a low-privilege Windows account. Your apps keep going when you close this window.</p>
        </div>

        <div className="sidebar-bottom">
          <button className="help-link" onClick={() => void openUrl("https://developer.microsoft.com/en-us/microsoft-edge/webview2/").catch((error: unknown) => notify(errorMessage(error), "error"))}>
            <CircleHelp aria-hidden="true" size={15} /> Help &amp; requirements
            <ArrowUpRight aria-hidden="true" size={13} />
          </button>
          <div className="build-label"><span className="build-dot" /> STAYUP 0.1.0 <span>·</span> WINSW 2.12</div>
        </div>
      </aside>

      <main id="home" className="main-content">
        <header className="topbar">
          <div className="breadcrumbs"><span>Workspace</span><span className="breadcrumb-slash">/</span><strong>Managed apps</strong></div>
          <div className="topbar-actions">
            <span className="privacy-mark"><ShieldCheck aria-hidden="true" size={14} /> Local to this PC</span>
            <button className="icon-button refresh-button" title="Refresh app statuses" aria-label="Refresh app statuses" onClick={() => void refresh(true)} disabled={refreshing}>
              <RefreshCw aria-hidden="true" className={refreshing ? "spin" : ""} size={16} />
            </button>
            <button className="button button-primary top-add-button" onClick={() => setEditing("new")}>
              <Plus aria-hidden="true" size={16} /> Add app
            </button>
          </div>
        </header>

        <div className="page-body">
          <div className="page-intro">
            <div className="intro-copy">
              <p className="eyebrow"><Sparkles aria-hidden="true" size={13} /> YOUR BACKGROUND WORKSPACE</p>
              <h1>Keep good things<br /><span>running.</span></h1>
              <p className="intro-description">Give the tools you rely on a quiet place to work in the background.</p>
            </div>
            <div className="intro-art" aria-hidden="true">
              <div className="orbit orbit-outer" />
              <div className="orbit orbit-inner" />
              <div className="art-card art-card-back"><span /><i /></div>
              <div className="art-card art-card-front"><div className="art-mark"><span /><span /><span /></div><i /><i /><i /></div>
              <span className="art-spark art-spark-one" />
              <span className="art-spark art-spark-two">·</span>
            </div>
          </div>

          <section className="stats-grid" aria-label="App overview">
            <StatCard icon={<Server aria-hidden="true" size={16} />} label="All apps" value={apps.length} tint="blue" />
            <StatCard icon={<Activity aria-hidden="true" size={16} />} label="Running now" value={runningCount} tint="green" />
            <StatCard icon={<ArrowDownUp aria-hidden="true" size={16} />} label="Stopped" value={stoppedCount} tint="amber" />
            <StatCard icon={<ShieldCheck aria-hidden="true" size={16} />} label="Need attention" value={needsAttention} tint={needsAttention ? "red" : "green"} />
          </section>

          <section className="managed-apps-section" aria-labelledby="managed-apps-heading">
            <div className="section-heading-line">
              <div>
                <p className="section-kicker">YOUR SERVICES</p>
                <h2 id="managed-apps-heading">Managed apps <span>{apps.length.toString().padStart(2, "0")}</span></h2>
              </div>
              {apps.length > 0 && (
                <button className="button button-secondary add-secondary" onClick={() => setEditing("new")}><Plus aria-hidden="true" size={15} /> Add an app</button>
              )}
            </div>

            {apps.length > 0 && (
              <div className="list-controls">
                <label className="search-control">
                  <Search aria-hidden="true" size={15} />
                  <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Find an app" aria-label="Find an app" />
                  {query && <button type="button" aria-label="Clear search" onClick={() => setQuery("")}><X size={14} /></button>}
                </label>
                <div className="filter-control">
                  <Filter aria-hidden="true" size={14} />
                  <label className="visually-hidden" htmlFor="app-filter">Filter apps</label>
                  <select id="app-filter" value={filter} onChange={(event) => setFilter(event.target.value as FilterMode)}>
                    <option value="all">All apps</option>
                    <option value="running">Running</option>
                    <option value="stopped">Stopped</option>
                    <option value="attention">Need attention</option>
                  </select>
                  <ChevronDown aria-hidden="true" size={13} />
                </div>
              </div>
            )}

            {loading ? (
              <div className="loading-panel" role="status"><span className="loading-orbit"><i /></span><strong>Getting things ready…</strong><small>Checking the apps you manage.</small></div>
            ) : !desktop ? (
              <div className="desktop-only-panel" role="status">
                <div className="desktop-only-icon"><Command aria-hidden="true" size={19} /></div>
                <div><strong>Ready to see your apps?</strong><p>Open StayUp as a Windows desktop app to manage background programs.</p></div>
              </div>
            ) : visibleApps.length === 0 && apps.length === 0 ? (
              <div className="empty-panel">
                <div className="empty-illustration" aria-hidden="true">
                  <span className="empty-ring" /><span className="empty-ring empty-ring-small" />
                  <div className="empty-window"><div /><i /><i /><i /></div>
                  <span className="empty-glint"><Sparkles size={13} /></span>
                </div>
                <p className="empty-kicker">A FRESH START</p>
                <h3>A little room to begin.</h3>
                <p className="empty-copy">Add an app you want to keep running. We’ll help it start with Windows, pick up after a crash, and save its output.</p>
                <button className="button button-primary empty-add" onClick={() => setEditing("new")}><Plus aria-hidden="true" size={16} /> Add your first app <ArrowUpRight aria-hidden="true" size={14} /></button>
                <div className="empty-footnote"><ShieldCheck aria-hidden="true" size={13} /> Your apps run locally, in the background.</div>
              </div>
            ) : visibleApps.length === 0 ? (
              <div className="no-results"><span className="no-results-icon"><Search size={16} /></span><strong>No apps match that search.</strong><p>Try a different name or change the filter.</p><button className="button button-quiet button-small" onClick={() => { setQuery(""); setFilter("all"); }}>Clear filters</button></div>
            ) : (
              <div className="app-list">
                <div className="app-list-labels"><span>APPLICATION</span><span>QUICK ACTIONS</span></div>
                {visibleApps.map((overview) => (
                  <AppCard
                    key={overview.app.id}
                    overview={overview}
                    onEdit={(app) => setEditing(app)}
                    onLogs={setLogsFor}
                    onAction={(action, app) => void runAction(action, app)}
                    onRemove={(app) => { setRemoving(app); setRemoveLogs(false); }}
                    onUpdated={(updated) => setApps((current) => current.map((item) => item.app.id === updated.app.id ? updated : item))}
                    onError={(message) => notify(message, "error")}
                  />
                ))}
              </div>
            )}
          </section>

          <footer className="page-footer">
            <span>Made for the things you keep running.</span>
            <span><ShieldCheck aria-hidden="true" size={13} /> Your computer. Your apps. Your rules.</span>
          </footer>
        </div>
      </main>

      <div className="toast-stack" aria-label="Notifications">
        {notices.map((notice) => <Toast key={notice.id} notice={notice} dismiss={(id) => setNotices((current) => current.filter((item) => item.id !== id))} />)}
      </div>

      {editing !== null && (
        <AppForm
          app={editing === "new" ? undefined : editing}
          onSave={saveApp}
          onClose={() => setEditing(null)}
        />
      )}
      {logsFor && <LogDialog app={logsFor} onClose={() => setLogsFor(null)} onError={(message) => notify(message, "error")} />}
      {removing && (
        <DialogFrame
          eyebrow="REMOVE APP"
          title={`Remove ${removing.name}?`}
          description="StayUp will stop this app and remove its background-service files. Your original program stays where it is."
          onClose={() => setRemoving(null)}
          width="narrow"
          footer={(
            <>
              <button className="button button-quiet" type="button" onClick={() => setRemoving(null)}>Keep app</button>
              <button className="button button-danger" type="button" onClick={() => void confirmRemove()}>Remove app</button>
            </>
          )}
        >
          <label className="remove-logs-option">
            <input type="checkbox" checked={removeLogs} onChange={(event) => setRemoveLogs(event.target.checked)} />
            Also remove this app’s saved logs
          </label>
        </DialogFrame>
      )}
    </div>
  );
}

function StatCard({ icon, label, value, tint }: { icon: React.ReactNode; label: string; value: number; tint: string }) {
  return (
    <div className="stat-card">
      <span className={`stat-icon stat-${tint}`}>{icon}</span>
      <span className="stat-label">{label}</span>
      <strong>{value.toString().padStart(2, "0")}</strong>
      <Check className={`stat-check stat-check-${tint}`} aria-hidden="true" size={14} />
    </div>
  );
}
