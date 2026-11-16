import { BellRing, CalendarClock } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type { Task } from "../types";
import {
  deadlineDescription,
  deadlineTone,
  formatDateTime,
  offsetOptions,
  parseLocalDate,
  toLocalInputValue,
} from "../lib/format";
import { Button, Field, Tag } from "./Primitives";
import { Modal } from "./Modal";

type ReminderMode = "countdown" | "absolute";

export function TaskReminderDialog({
  task,
  onClose,
  onSubmit,
}: {
  task: Task;
  onClose: () => void;
  onSubmit: (input: {
    mode: ReminderMode;
    offset_minutes?: number;
    remind_at?: string;
  }) => Promise<void>;
}) {
  const hasDeadline = Boolean(task.deadline);
  const [mode, setMode] = useState<ReminderMode>(hasDeadline ? "countdown" : "absolute");
  const [offset, setOffset] = useState(60);
  const [time, setTime] = useState(() => {
    const deadline = parseLocalDate(task.deadline);
    const fallback = deadline && deadline.getTime() > Date.now() + 3_600_000
      ? deadline.getTime() - 3_600_000
      : Date.now() + 3_600_000;
    return toLocalInputValue(new Date(fallback));
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const tone = deadlineTone(task.deadline);

  useEffect(() => {
    if (!hasDeadline) setMode("absolute");
  }, [hasDeadline]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    if (mode === "absolute" && !time) {
      setError("请选择提醒日期和时间");
      return;
    }
    setBusy(true);
    try {
      await onSubmit(
        mode === "countdown"
          ? { mode, offset_minutes: offset }
          : { mode, remind_at: time },
      );
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "提醒创建失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title="设置任务提醒"
      description={task.title}
      onClose={onClose}
    >
      <form onSubmit={submit}>
        <div className="modal-body">
          <div className={`task-reminder-summary tone-${tone}`}>
            <CalendarClock size={19} />
            <div>
              <strong>截止时间：{formatDateTime(task.deadline)}</strong>
              <small>{deadlineDescription(task.deadline)}</small>
            </div>
            <Tag tone={tone === "danger" ? "danger" : tone === "warning" ? "warning" : "neutral"}>
              {task.course_name || "学习任务"}
            </Tag>
          </div>
          <div className="toolbar-segment reminder-mode" role="group" aria-label="提醒方式">
            <button
              type="button"
              className={mode === "countdown" ? "selected" : ""}
              onClick={() => setMode("countdown")}
              disabled={!hasDeadline}
            >
              截止前倒计时
            </button>
            <button
              type="button"
              className={mode === "absolute" ? "selected" : ""}
              onClick={() => setMode("absolute")}
            >
              指定日期时间
            </button>
          </div>
          {mode === "countdown" ? (
            <Field label="提前多久提醒" hint="按任务截止时间计算，提醒时间必须晚于现在">
              <select value={offset} onChange={(event) => setOffset(Number(event.target.value))}>
                {offsetOptions.map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </Field>
          ) : (
            <Field label="提醒日期和时间">
              <input
                type="datetime-local"
                value={time}
                onChange={(event) => setTime(event.target.value)}
                required
              />
            </Field>
          )}
          {error && <p className="form-error">{error}</p>}
        </div>
        <footer className="modal-actions">
          <Button type="button" variant="secondary" onClick={onClose} disabled={busy}>
            取消
          </Button>
          <Button
            type="submit"
            variant="primary"
            icon={<BellRing size={16} />}
            disabled={busy}
          >
            {busy ? "正在创建…" : "创建提醒"}
          </Button>
        </footer>
      </form>
    </Modal>
  );
}
