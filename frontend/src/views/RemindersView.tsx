import {
  BellRing,
  CalendarClock,
  Clock3,
  Pause,
  Play,
  Repeat2,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import type { Reminder, RepeatRule, WorkspaceData } from "../types";
import {
  accountName,
  formatDateTime,
  repeatRuleLabels,
  toLocalInputValue,
} from "../lib/format";
import { Button, EmptyState, Field, IconButton, Tag } from "../components/Primitives";
import { platformLabel } from "../components/AppShell";

export function RemindersView({
  data,
  onCreate,
  onToggle,
  onDelete,
}: {
  data: WorkspaceData;
  onCreate: (input: { title: string; remind_at: string; repeat_rule: RepeatRule }) => Promise<void>;
  onToggle: (reminder: Reminder) => Promise<void>;
  onDelete: (reminder: Reminder) => Promise<void>;
}) {
  const active = data.reminders.filter((reminder) => reminder.enabled);
  const linked = active.filter((reminder) => reminder.task_id).length;
  const routine = active.filter((reminder) => reminder.repeat_rule !== "once").length;

  return (
    <div className="reminders-view">
      <section className="list-intro reminder-intro">
        <div>
          <span className="eyebrow">REMINDER PLAN</span>
          <h2>把准备时间留在前面</h2>
          <p>
            {active.length
              ? `${active.length} 个提醒正在运行，${linked} 个关联学习任务。`
              : "为课程、作业或每周学习安排一个固定提醒。"}
          </p>
        </div>
        <div className="reminder-metrics">
          <div>
            <BellRing size={17} />
            <span>运行中</span>
            <strong>{active.length}</strong>
          </div>
          <div>
            <Repeat2 size={17} />
            <span>重复计划</span>
            <strong>{routine}</strong>
          </div>
        </div>
      </section>

      <div className="reminder-layout">
        <ReminderForm onCreate={onCreate} />
        <section className="surface reminder-board">
          <header className="section-header">
            <div>
              <span className="section-kicker">SCHEDULE</span>
              <h3>提醒计划</h3>
            </div>
            <Tag tone={active.length ? "success" : "neutral"}>
              {active.length} 个已启用
            </Tag>
          </header>
          {data.reminders.length ? (
            <div className="reminder-list">
              {data.reminders.map((reminder) => (
                <ReminderRow
                  key={reminder.id}
                  reminder={reminder}
                  platforms={data.platforms}
                  accounts={data.accounts}
                  onToggle={() => onToggle(reminder)}
                  onDelete={() => onDelete(reminder)}
                />
              ))}
            </div>
          ) : (
            <EmptyState
              icon={<BellRing size={29} />}
              title="还没有提醒计划"
              description="用左侧表单指定提醒内容与时间，到点后会在本机通知你。"
            />
          )}
        </section>
      </div>
    </div>
  );
}

function ReminderForm({
  onCreate,
}: {
  onCreate: (input: { title: string; remind_at: string; repeat_rule: RepeatRule }) => Promise<void>;
}) {
  const [title, setTitle] = useState("");
  const [time, setTime] = useState(() => toLocalInputValue());
  const [repeat, setRepeat] = useState<RepeatRule>("once");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!time) setTime(toLocalInputValue());
  }, [time]);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    if (!title.trim()) {
      setError("请填写提醒内容");
      return;
    }
    if (!time) {
      setError("请选择提醒时间");
      return;
    }
    setBusy(true);
    try {
      await onCreate({ title: title.trim(), remind_at: time, repeat_rule: repeat });
      setTitle("");
      setTime(toLocalInputValue());
      setRepeat("once");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "提醒创建失败");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="surface reminder-form">
      <header className="section-header">
        <div>
          <span className="section-kicker">NEW REMINDER</span>
          <h3>安排新提醒</h3>
        </div>
        <span className="section-note">
          <Sparkles size={15} />
          本机通知
        </span>
      </header>
      <form className="form-stack" onSubmit={submit}>
        <Field label="提醒内容">
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="例如：复习 Unit 3 的听力练习"
            required
          />
        </Field>
        <Field label="提醒时间">
          <input
            type="datetime-local"
            value={time}
            onChange={(event) => setTime(event.target.value)}
            required
          />
        </Field>
        <Field label="重复方式">
          <select
            value={repeat}
            onChange={(event) => setRepeat(event.target.value as RepeatRule)}
          >
            {(Object.keys(repeatRuleLabels) as RepeatRule[]).map((rule) => (
              <option key={rule} value={rule}>
                {repeatRuleLabels[rule]}
              </option>
            ))}
          </select>
        </Field>
        <Button
          type="submit"
          variant="primary"
          icon={<BellRing size={16} />}
          disabled={busy}
        >
          {busy ? "正在创建…" : "创建提醒"}
        </Button>
        {error && <p className="form-error">{error}</p>}
      </form>
      <p className="reminder-tip">
        <CalendarClock size={16} />
        想在作业截止前提醒，可以从任务列表打开任务提醒。
      </p>
    </section>
  );
}

function ReminderRow({
  reminder,
  platforms,
  accounts,
  onToggle,
  onDelete,
}: {
  reminder: Reminder;
  platforms: WorkspaceData["platforms"];
  accounts: WorkspaceData["accounts"];
  onToggle: () => Promise<void>;
  onDelete: () => Promise<void>;
}) {
  const [toggleBusy, setToggleBusy] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const account = accounts.find((item) => item.id === reminder.account_id);
  const subtitle =
    reminder.platform && reminder.account_id
      ? `${platformLabel(reminder.platform, platforms)} · ${accountName(account)}`
      : reminder.repeat_rule === "once"
        ? "一次性个人提醒"
        : `固定计划 · ${repeatRuleLabels[reminder.repeat_rule]}`;
  return (
    <article className={`reminder-row ${reminder.enabled ? "" : "disabled"}`}>
      <span className="reminder-row-icon">
        <BellRing size={18} />
      </span>
      <div className="reminder-main">
        <strong>{reminder.title || reminder.task_title || "学习提醒"}</strong>
        <small>{subtitle}</small>
      </div>
      <div className="reminder-time">
        <span>
          <Clock3 size={14} />
          {formatDateTime(reminder.remind_at)}
        </span>
        <Tag tone={reminder.enabled ? "accent" : "neutral"}>
          {repeatRuleLabels[reminder.repeat_rule]}
        </Tag>
      </div>
      <div className="reminder-actions">
        <IconButton
          label={reminder.enabled ? "停用提醒" : "启用提醒"}
          disabled={toggleBusy}
          onClick={async () => {
            setToggleBusy(true);
            try {
              await onToggle();
            } finally {
              setToggleBusy(false);
            }
          }}
        >
          {reminder.enabled ? <Pause size={17} /> : <Play size={17} />}
        </IconButton>
        <IconButton
          label="删除提醒"
          className="danger-icon"
          disabled={deleteBusy}
          onClick={async () => {
            setDeleteBusy(true);
            try {
              await onDelete();
            } finally {
              setDeleteBusy(false);
            }
          }}
        >
          <Trash2 size={17} />
        </IconButton>
      </div>
    </article>
  );
}
