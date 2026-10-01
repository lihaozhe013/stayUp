import { useEffect, useId, useRef } from "react";
import type { ReactNode } from "react";
import { X } from "lucide-react";

interface DialogFrameProps {
  title: string;
  eyebrow?: string;
  description?: string;
  children: ReactNode;
  onClose: () => void;
  width?: "wide" | "narrow";
  footer?: ReactNode;
}

export function DialogFrame({ title, eyebrow, description, children, onClose, width = "wide", footer }: DialogFrameProps) {
  const dialog = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (!element.open) element.showModal();
    return () => {
      if (element.open) element.close();
    };
  }, []);

  return (
    <dialog
      ref={dialog}
      className={`dialog-shell dialog-${width}`}
      aria-labelledby={titleId}
      aria-describedby={description ? descriptionId : undefined}
      onClose={onClose}
    >
      <div className="dialog-heading">
        <div>
          {eyebrow && <p className="dialog-eyebrow">{eyebrow}</p>}
          <h2 id={titleId}>{title}</h2>
          {description && <p id={descriptionId} className="dialog-description">{description}</p>}
        </div>
        <button className="icon-button dialog-close" aria-label="Close" onClick={() => dialog.current?.close()}>
          <X aria-hidden="true" size={19} />
        </button>
      </div>
      <div className="dialog-content">{children}</div>
      {footer && <div className="dialog-footer">{footer}</div>}
    </dialog>
  );
}
