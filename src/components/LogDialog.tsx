import { useEffect, useState } from "react";
import { Clipboard, ClipboardCheck, FileOutput, FolderOpen, RefreshCw } from "lucide-react";
import type { LogResponse, LogStream, ManagedApp } from "../bindings";
import { openLogFolder, readAppLogs } from "../api";
import { DialogFrame } from "./DialogFrame";

interface LogDialogProps {
  app: ManagedApp;
  onClose: () => void;
  onError: (message: string) => void;
}

const streams: { id: LogStream; label: string }[] = [
  { id: "stdout", label: "Output" },
  { id: "stderr", label: "Errors" },
  { id: "diagnostic", label: "StayUp" },
];

export function LogDialog({ app, onClose, onError }: LogDialogProps) {
  const [stream, setStream] = useState<LogStream>("stdout");
  const [response, setResponse] = useState<LogResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [copied, setCopied] = useState(false);

  async function refresh() {
    setLoading(true);
    try {
      setResponse(await readAppLogs(app.id, stream));
    } catch (error) {
      onError(error instanceof Error ? error.message : "Could not read this log.");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    // Fetch again when the user selects a different stream.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stream, app.id]);

  async function copyLog() {
    if (!response) return;
    try {
      await navigator.clipboard.writeText(response.content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      onError("Could not copy the log to the clipboard.");
    }
  }

  async function openFolder() {
    try {
      await openLogFolder(app.id);
    } catch (error) {
      onError(error instanceof Error ? error.message : "Could not open the log folder.");
    }
  }

  return (
    <DialogFrame
      eyebrow="APP OUTPUT"
      title={`${app.name} logs`}
      description="A snapshot of the latest output from this background app."
      width="wide"
      onClose={onClose}
      footer={(
        <>
          <button className="button button-quiet" type="button" onClick={() => void openFolder()}><FolderOpen size={15} /> Open log folder</button>
          <button className="button button-secondary" type="button" onClick={() => void copyLog()} disabled={!response || !response.content}>
            {copied ? <ClipboardCheck size={15} /> : <Clipboard size={15} />}{copied ? "Copied" : "Copy log"}
          </button>
          <button className="button button-secondary" type="button" onClick={() => void refresh()} disabled={loading}>
            <RefreshCw className={loading ? "spin" : ""} size={15} /> Refresh
          </button>
        </>
      )}
    >
      <div className="log-toolbar">
        <div className="log-tabs" role="tablist" aria-label="Log stream">
          {streams.map((option) => (
            <button
              className={`log-tab ${stream === option.id ? "is-active" : ""}`}
              role="tab"
              type="button"
              aria-selected={stream === option.id}
              key={option.id}
              onClick={() => setStream(option.id)}
            ><FileOutput aria-hidden="true" size={14} /> {option.label}</button>
          ))}
        </div>
        {response?.truncated && <span className="log-limit-note">Showing the latest 1 MiB</span>}
      </div>
      <pre className={`log-output ${loading ? "is-loading" : ""}`} aria-live="polite">
        {loading ? "Loading the latest output…" : response?.content || "No output has been captured for this stream yet."}
      </pre>
    </DialogFrame>
  );
}
