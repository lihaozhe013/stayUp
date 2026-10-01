import {
  ArrowUpRight,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  FileCode2,
  FolderOpen,
  Plus,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { AppTemplate, ManagedApp } from "../bindings";
import { templateDefaults, validateAppDraft } from "../api";
import type { AppDraft } from "../api";
import { DialogFrame } from "./DialogFrame";

interface AppFormProps {
  app?: ManagedApp;
  onSave: (draft: AppDraft, startNow: boolean) => Promise<void>;
  onClose: () => void;
}

const templates: { id: AppTemplate; name: string; description: string; mark: string }[] = [
  { id: "backgroundApplication", name: "Background app", description: "A dependable home for the apps you keep running.", mark: "01" },
  { id: "localServer", name: "Local server", description: "For local servers, game servers, and APIs.", mark: "02" },
  { id: "backgroundWorker", name: "Background worker", description: "For bots, workers, and sync tools.", mark: "03" },
  { id: "custom", name: "Custom", description: "Fine-tune each setting to fit your workflow.", mark: "04" },
];

const makeDraft = (app?: ManagedApp): AppDraft => app ? {
  name: app.name,
  process: structuredClone(app.process),
  startupEnabled: app.startupEnabled,
  restart: { ...app.restart },
  loggingEnabled: app.loggingEnabled,
  advanced: { ...app.advanced },
} : {
  name: "",
  process: { executable: "", arguments: "", workingDirectory: "", environment: [] },
  ...templateDefaults.backgroundApplication,
};

function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return "Could not save this app. Check the settings and try again.";
}

export function AppForm({ app, onSave, onClose }: AppFormProps) {
  const [draft, setDraft] = useState<AppDraft>(() => makeDraft(app));
  const [selectedTemplate, setSelectedTemplate] = useState<AppTemplate>(app ? "custom" : "backgroundApplication");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [startNow, setStartNow] = useState(!app);
  const [saving, setSaving] = useState(false);
  const [inlineError, setInlineError] = useState("");

  const executableDirectory = useMemo(() => {
    const path = draft.process.executable;
    const lastSeparator = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
    return lastSeparator >= 0 ? path.slice(0, lastSeparator) : "";
  }, [draft.process.executable]);

  function setProcess<K extends keyof AppDraft["process"]>(key: K, value: AppDraft["process"][K]) {
    setDraft((current) => ({ ...current, process: { ...current.process, [key]: value } }));
  }

  function chooseTemplate(template: AppTemplate) {
    setSelectedTemplate(template);
    setDraft((current) => ({ ...current, ...templateDefaults[template] }));
  }

  async function chooseExecutable() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Windows applications", extensions: ["exe"] }],
      });
      if (typeof selected === "string") {
        const lastSeparator = Math.max(selected.lastIndexOf("\\"), selected.lastIndexOf("/"));
        setDraft((current) => ({
          ...current,
          process: {
            ...current.process,
            executable: selected,
            workingDirectory: current.process.workingDirectory || selected.slice(0, lastSeparator),
          },
          name: current.name || selected.slice(lastSeparator + 1).replace(/\.exe$/i, ""),
        }));
      }
    } catch (error) {
      setInlineError(error instanceof Error ? error.message : "Could not open the file picker.");
    }
  }

  async function chooseWorkingDirectory() {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Choose a working folder" });
      if (typeof selected === "string") setProcess("workingDirectory", selected);
    } catch (error) {
      setInlineError(error instanceof Error ? error.message : "Could not open the folder picker.");
    }
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setInlineError("");
    if (!draft.name.trim()) {
      setInlineError("Give this app a name before saving.");
      return;
    }
    if (!draft.process.executable.trim()) {
      setInlineError("Choose an executable before saving.");
      return;
    }
    if (!draft.process.workingDirectory.trim()) {
      setInlineError("Choose a working folder before saving.");
      return;
    }

    setSaving(true);
    try {
      await validateAppDraft(draft);
      await onSave(draft, startNow);
    } catch (error) {
      setInlineError(errorText(error));
      setSaving(false);
    }
  }

  return (
    <DialogFrame
      eyebrow={app ? "MANAGED APP" : "NEW BACKGROUND APP"}
      title={app ? "App settings" : "Give your app a steady home."}
      description={app ? "Adjust the way this app starts and recovers." : "Choose a program, set its routine, and let it run quietly in the background."}
      onClose={onClose}
      width="wide"
      footer={(
        <>
          <button className="button button-quiet" type="button" onClick={onClose}>Cancel</button>
          <button className="button button-primary form-submit" form="app-settings-form" disabled={saving}>
            {saving ? "Saving…" : app ? "Save changes" : "Create background app"}
            {!saving && <ArrowUpRight aria-hidden="true" size={15} />}
          </button>
        </>
      )}
    >
      <form id="app-settings-form" className="app-form" onSubmit={save}>
        {inlineError && (
          <div className="form-error" role="alert">
            <ShieldCheck aria-hidden="true" size={16} />
            <span>{inlineError}</span>
          </div>
        )}

        {!app && (
          <fieldset className="template-section">
            <legend>Start with a template</legend>
            <div className="template-grid">
              {templates.map((template) => (
                <button
                  className={`template-option ${selectedTemplate === template.id ? "is-selected" : ""}`}
                  type="button"
                  key={template.id}
                  aria-pressed={selectedTemplate === template.id}
                  onClick={() => chooseTemplate(template.id)}
                >
                  <span className="template-mark">{template.mark}</span>
                  <span className="template-copy">
                    <strong>{template.name}</strong>
                    <small>{template.description}</small>
                  </span>
                  {selectedTemplate === template.id && <CircleCheck aria-label="Selected" size={17} />}
                </button>
              ))}
            </div>
          </fieldset>
        )}

        <section className="form-section program-section" aria-labelledby="program-heading">
          <div className="form-section-title">
            <span className="section-index">01</span>
            <div>
              <h3 id="program-heading">Pick your program</h3>
              <p>Use a local Windows executable and its normal command-line arguments.</p>
            </div>
          </div>
          <label className="field-label" htmlFor="app-name">App name</label>
          <input
            id="app-name"
            className="text-input"
            value={draft.name}
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
            maxLength={80}
            placeholder="e.g. My local server"
            required
          />
          <label className="field-label spaced-label" htmlFor="executable-path">Executable file</label>
          <div className="path-picker-row">
            <input
              id="executable-path"
              className="text-input path-input"
              value={draft.process.executable}
              onChange={(event) => setProcess("executable", event.target.value)}
              placeholder="Choose a local .exe file"
              autoComplete="off"
              required
            />
            <button type="button" className="button button-secondary browse-button" onClick={() => void chooseExecutable()}>
              <FolderOpen aria-hidden="true" size={15} /> Browse
            </button>
          </div>
          <label className="field-label spaced-label" htmlFor="program-arguments">Arguments <span className="optional-label">optional</span></label>
          <input
            id="program-arguments"
            className="text-input code-input"
            value={draft.process.arguments}
            onChange={(event) => setProcess("arguments", event.target.value)}
            placeholder={'--port 8080 --config "C:\\Apps\\service.toml"'}
            autoComplete="off"
          />
          <p className="field-hint"><FileCode2 aria-hidden="true" size={13} /> Arguments are passed directly to the program. Quote values that contain spaces.</p>
        </section>

        <div className="form-columns">
          <section className="form-section behavior-section" aria-labelledby="behavior-heading">
            <div className="form-section-title">
              <span className="section-index">02</span>
              <div>
                <h3 id="behavior-heading">Set the routine</h3>
                <p>Choose what happens when Windows starts or the app exits.</p>
              </div>
            </div>
            <ToggleRow
              checked={draft.startupEnabled}
              onChange={(checked) => setDraft({ ...draft, startupEnabled: checked })}
              title="Start automatically with Windows"
              description="Run quietly in the background after your PC starts."
            />
            <ToggleRow
              checked={draft.restart.policy === "onFailure"}
              onChange={(checked) => setDraft({
                ...draft,
                restart: { ...draft.restart, policy: checked ? "onFailure" : "never" },
              })}
              title="Restart the app if it crashes"
              description="A clean exit or a manual stop stays stopped."
            />
            {draft.restart.policy === "onFailure" && (
              <label className="compact-setting" htmlFor="restart-delay">
                <span>Wait before restarting</span>
                <span className="select-wrap">
                  <select
                    id="restart-delay"
                    className="compact-select"
                    value={draft.restart.delaySeconds}
                    onChange={(event) => setDraft({
                      ...draft,
                      restart: { ...draft.restart, delaySeconds: Number(event.target.value) },
                    })}
                  >
                    <option value={1}>1 second</option>
                    <option value={5}>5 seconds</option>
                    <option value={10}>10 seconds</option>
                    <option value={30}>30 seconds</option>
                    <option value={60}>1 minute</option>
                  </select>
                  <ChevronDown aria-hidden="true" size={14} />
                </span>
              </label>
            )}
            <ToggleRow
              checked={draft.loggingEnabled}
              onChange={(checked) => setDraft({ ...draft, loggingEnabled: checked })}
              title="Save program output to logs"
              description="Capture stdout and stderr with simple automatic rotation."
              last
            />
            {!app && (
              <ToggleRow
                checked={startNow}
                onChange={setStartNow}
                title="Start after creating"
                description="You can always start this app later."
                last
              />
            )}
          </section>
        </div>

        <section className="advanced-section">
          <button
            type="button"
            className="advanced-toggle"
            aria-expanded={advancedOpen}
            onClick={() => setAdvancedOpen(!advancedOpen)}
          >
            <span className="advanced-toggle-left">
              {advancedOpen ? <ChevronDown aria-hidden="true" size={17} /> : <ChevronRight aria-hidden="true" size={17} />}
              <SlidersMark />
              <span><strong>Advanced settings</strong><small>Working folder, environment variables, and stop time</small></span>
            </span>
            <Sparkles aria-hidden="true" size={15} />
          </button>
          {advancedOpen && (
            <div className="advanced-panel">
              <label className="field-label" htmlFor="working-directory">Working folder</label>
              <div className="path-picker-row">
                <input
                  id="working-directory"
                  className="text-input path-input"
                  value={draft.process.workingDirectory}
                  onChange={(event) => setProcess("workingDirectory", event.target.value)}
                  placeholder={executableDirectory || "Choose the program folder"}
                  autoComplete="off"
                  required
                />
                <button type="button" className="button button-secondary browse-button" onClick={() => void chooseWorkingDirectory()}>
                  <FolderOpen aria-hidden="true" size={15} /> Browse
                </button>
              </div>
              <p className="field-hint">Defaults to the executable folder. The LocalService account must be able to access it.</p>

              <div className="environment-heading">
                <div><label className="field-label" htmlFor="environment-name-0">Environment variables</label><span className="optional-label">optional</span></div>
                <button
                  type="button"
                  className="button button-quiet button-small"
                  onClick={() => setProcess("environment", [...draft.process.environment, { name: "", value: "" }])}
                ><Plus aria-hidden="true" size={14} /> Add variable</button>
              </div>
              {draft.process.environment.map((variable, index) => (
                <div className="environment-row" key={`${index}-${variable.name}`}>
                  <input
                    id={index === 0 ? "environment-name-0" : undefined}
                    className="text-input code-input"
                    aria-label={`Environment variable ${index + 1} name`}
                    placeholder="NAME"
                    value={variable.name}
                    onChange={(event) => setProcess("environment", draft.process.environment.map((item, position) => position === index ? { ...item, name: event.target.value } : item))}
                  />
                  <input
                    className="text-input code-input"
                    aria-label={`Environment variable ${index + 1} value`}
                    placeholder="Value"
                    value={variable.value}
                    onChange={(event) => setProcess("environment", draft.process.environment.map((item, position) => position === index ? { ...item, value: event.target.value } : item))}
                  />
                  <button
                    type="button"
                    className="icon-button remove-variable"
                    aria-label={`Remove environment variable ${index + 1}`}
                    onClick={() => setProcess("environment", draft.process.environment.filter((_, position) => position !== index))}
                  ><Trash2 aria-hidden="true" size={15} /></button>
                </div>
              ))}
              <label className="field-label spaced-label" htmlFor="stop-timeout">Wait before force stopping</label>
              <div className="path-picker-row timeout-row">
                <input
                  id="stop-timeout"
                  className="text-input timeout-input"
                  type="number"
                  min={0}
                  max={600}
                  value={draft.advanced.stopTimeoutSeconds}
                  onChange={(event) => setDraft({ ...draft, advanced: { stopTimeoutSeconds: Number(event.target.value) } })}
                />
                <span className="field-unit">seconds</span>
              </div>
              <div className="account-note"><ShieldCheck aria-hidden="true" size={16} /><span>Runs under the restricted LocalService account, outside your sign-in. Files inside your user profile may not be available.</span></div>
            </div>
          )}
        </section>
      </form>
    </DialogFrame>
  );
}

function ToggleRow({ checked, onChange, title, description, last = false }: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  title: string;
  description: string;
  last?: boolean;
}) {
  return (
    <label className={`toggle-row ${last ? "toggle-row-last" : ""}`}>
      <span className="toggle-copy"><strong>{title}</strong><small>{description}</small></span>
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
    </label>
  );
}

function SlidersMark() {
  return <span className="sliders-mark" aria-hidden="true"><i /><i /><i /></span>;
}
