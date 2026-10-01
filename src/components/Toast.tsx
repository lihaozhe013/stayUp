import { AlertCircle, Check, X } from "lucide-react";

export interface Notice {
  id: number;
  message: string;
  tone: "success" | "error";
}

interface ToastProps {
  notice: Notice;
  dismiss: (id: number) => void;
}

export function Toast({ notice, dismiss }: ToastProps) {
  const StatusIcon = notice.tone === "success" ? Check : AlertCircle;
  return (
    <div className={`toast toast-${notice.tone}`} role="status">
      <StatusIcon aria-hidden="true" size={16} />
      <span>{notice.message}</span>
      <button aria-label="Dismiss notification" onClick={() => dismiss(notice.id)}>
        <X aria-hidden="true" size={15} />
      </button>
    </div>
  );
}
