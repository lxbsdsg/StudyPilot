import { AlertCircle, CheckCircle2, LoaderCircle, X } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

export type Tone = "neutral" | "success" | "warning" | "danger" | "accent";

export function Button({
  variant = "secondary",
  size = "normal",
  icon,
  children,
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "normal" | "small" | "icon";
  icon?: ReactNode;
}) {
  return (
    <button
      className={`button button-${variant} button-${size} ${className}`}
      {...props}
    >
      {icon}
      {children}
    </button>
  );
}

export function IconButton({
  label,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  children: ReactNode;
}) {
  return (
    <button className="icon-button" aria-label={label} title={label} {...props}>
      {children}
    </button>
  );
}

export function Tag({
  tone = "neutral",
  children,
  dot = false,
}: {
  tone?: Tone;
  children: ReactNode;
  dot?: boolean;
}) {
  return (
    <span className={`tag tag-${tone}`}>
      {dot && <span className="tag-dot" />}
      {children}
    </span>
  );
}

export function StatusMark({
  status,
}: {
  status: string;
}) {
  if (status === "ready") return <Tag tone="success" dot>正常</Tag>;
  if (status === "refreshing") return <Tag tone="accent" dot>同步中</Tag>;
  return <Tag tone="danger" dot>需要处理</Tag>;
}

export function LoadingState({ label = "正在加载学习数据" }: { label?: string }) {
  return (
    <div className="loading-state" role="status">
      <LoaderCircle className="spin" size={22} strokeWidth={1.8} />
      <span>{label}</span>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon}</div>
      <strong>{title}</strong>
      <p>{description}</p>
      {action}
    </div>
  );
}

export function ErrorState({
  message,
  onRetry,
}: {
  message?: string;
  onRetry: () => void;
}) {
  return (
    <div className="error-banner" role="alert">
      <AlertCircle size={18} />
      <span>{message || "无法连接到本地服务，请确认 StudyPilot 服务已启动。"}</span>
      <Button variant="secondary" size="small" onClick={onRetry}>
        重试
      </Button>
    </div>
  );
}

export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint && <span className="field-hint">{hint}</span>}
    </label>
  );
}

export function Toast({
  message,
  kind,
  onClose,
}: {
  message: string;
  kind: "success" | "error" | "info";
  onClose: () => void;
}) {
  return (
    <div className={`toast toast-${kind}`} role="status">
      {kind === "success" ? (
        <CheckCircle2 size={18} />
      ) : kind === "error" ? (
        <AlertCircle size={18} />
      ) : (
        <LoaderCircle size={18} className={kind === "info" ? "spin" : ""} />
      )}
      <span>{message}</span>
      <IconButton label="关闭提示" onClick={onClose}>
        <X size={16} />
      </IconButton>
    </div>
  );
}
