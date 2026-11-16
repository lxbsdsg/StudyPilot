import {
  ArrowUpDown,
  BellPlus,
  Check,
  CheckCircle2,
  Clock3,
  ExternalLink,
  EyeOff,
  FileSearch,
  Filter,
  ListFilter,
  Search,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { Task, TaskStatus, WorkspaceData } from "../types";
import {
  accountName,
  deadlineDescription,
  deadlineTone,
  filterAndSortTasks,
  formatDateTime,
  platformTone,
  type TaskFilter,
  type TaskSortMode,
} from "../lib/format";
import { Button, EmptyState, Field, Tag } from "../components/Primitives";
import {
  platformInitial,
  platformLabel,
} from "../components/AppShell";
export function TasksView({
  data,
  onOpenTask,
  onStatus,
  onRemind,
  onLookup,
}: {
  data: WorkspaceData;
  onOpenTask: (task: Task) => void;
  onStatus: (task: Task, status: TaskStatus) => void;
  onRemind: (task: Task) => void;
  onLookup: (accountId: string, essayId: string) => Promise<void>;
}) {
  const [platformFilter, setPlatformFilter] = useState("");
  const [accountFilter, setAccountFilter] = useState("");
  const [statusFilter, setStatusFilter] = useState<TaskFilter>("pending");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<TaskSortMode>("deadline");
  const [mobileFilters, setMobileFilters] = useState(false);
  const [lookupOpen, setLookupOpen] = useState(false);
  const filtered = useMemo(
    () =>
      filterAndSortTasks(data.tasks, {
        status: statusFilter,
        platform: platformFilter,
        accountId: accountFilter,
        query,
        sort,
      }),
    [accountFilter, data.tasks, platformFilter, query, sort, statusFilter],
  );
  const activeFilters = [platformFilter, accountFilter, statusFilter !== "pending"].filter(Boolean).length;
  const pigaiAccounts = data.accounts.filter((account) => account.platform === "pigai");
  return (
    <div className="tasks-view">
      <section className="list-intro">
        <div>
          <span className="eyebrow">TASK BOARD</span>
          <h2>专注下一步</h2>
          <p>
            {data.tasks.filter((task) => task.status === "pending").length} 项任务等待处理，
            {data.dashboard.due_soon} 项在三天内截止。
          </p>
        </div>
        {pigaiAccounts.length > 0 && (
          <Button
            variant="secondary"
            icon={<FileSearch size={16} />}
            onClick={() => setLookupOpen(!lookupOpen)}
          >
            按作文号查询
          </Button>
        )}
      </section>
      {lookupOpen && (
        <PigaiLookup accounts={pigaiAccounts} onLookup={onLookup} />
      )}
      <section className="surface task-board">
        <div className="task-toolbar">
          <div className="search-box task-search">
            <Search size={17} />
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索课程、任务或来源"
              aria-label="搜索任务"
            />
            {query && (
              <button aria-label="清除搜索" onClick={() => setQuery("")}>
                <X size={15} />
              </button>
            )}
          </div>
          <div className="toolbar-segment" aria-label="任务状态">
            {[
              ["pending", "待完成"],
              ["all", "全部"],
              ["ignored", "已忽略"],
            ].map(([value, label]) => (
              <button
                key={value}
                className={statusFilter === value ? "selected" : ""}
                onClick={() => setStatusFilter(value as TaskFilter)}
              >
                {label}
              </button>
            ))}
          </div>
          <button
            className={`toolbar-button ${activeFilters ? "has-filter" : ""}`}
            onClick={() => setMobileFilters(!mobileFilters)}
          >
            <Filter size={16} />
            <span>筛选</span>
            {activeFilters > 0 && <b>{activeFilters}</b>}
          </button>
          <label className="sort-select">
            <ArrowUpDown size={15} />
            <select
              value={sort}
              onChange={(event) => setSort(event.target.value as TaskSortMode)}
              aria-label="任务排序"
            >
              <option value="deadline">截止时间</option>
              <option value="updated">最近更新</option>
              <option value="title">任务名称</option>
            </select>
          </label>
        </div>
        <div className={`filter-drawer ${mobileFilters ? "open" : ""}`}>
          <label>
            <span>学习平台</span>
            <select value={platformFilter} onChange={(event) => setPlatformFilter(event.target.value)}>
              <option value="">全部平台</option>
              {data.platforms.map((platform) => (
                <option key={platform.id} value={platform.id}>
                  {platform.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>学习账号</span>
            <select value={accountFilter} onChange={(event) => setAccountFilter(event.target.value)}>
              <option value="">全部账号</option>
              {data.accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {accountName(account)} · {platformLabel(account.platform, data.platforms)}
                </option>
              ))}
            </select>
          </label>
          <button
            className="clear-filters"
            onClick={() => {
              setPlatformFilter("");
              setAccountFilter("");
              setStatusFilter("pending");
            }}
          >
            重置筛选
          </button>
        </div>
        <div className="list-toolbar-caption">
          <span>
            <ListFilter size={15} />
            {filtered.length} 项结果
          </span>
          {activeFilters > 0 && (
            <button
              onClick={() => {
                setPlatformFilter("");
                setAccountFilter("");
                setStatusFilter("pending");
              }}
            >
              清除筛选
            </button>
          )}
        </div>
        {filtered.length ? (
          <>
            <div className="task-list-head">
              <span>任务</span>
              <span>课程与来源</span>
              <span>截止时间</span>
              <span>状态</span>
              <span>操作</span>
            </div>
            <div className="task-list">
              {filtered.map((task) => (
                <TaskRow
                  key={task.id}
                  task={task}
                  account={data.accounts.find((account) => account.id === task.account_id)}
                  platforms={data.platforms}
                  onOpen={() => onOpenTask(task)}
                  onStatus={onStatus}
                  onRemind={() => onRemind(task)}
                />
              ))}
            </div>
          </>
        ) : (
          <EmptyState
            icon={<CheckCircle2 size={28} />}
            title={query || activeFilters ? "没有符合条件的任务" : "当前没有待完成任务"}
            description={
              query || activeFilters
                ? "换一个关键词或清除筛选条件后再试。"
                : "同步学习账号后，新的课程与作业会出现在这里。"
            }
            action={
              query || activeFilters ? (
                <Button
                  size="small"
                  onClick={() => {
                    setQuery("");
                    setPlatformFilter("");
                    setAccountFilter("");
                    setStatusFilter("pending");
                  }}
                >
                  重置筛选
                </Button>
              ) : undefined
            }
          />
        )}
      </section>
    </div>
  );
}

function TaskRow({
  task,
  account,
  platforms,
  onOpen,
  onStatus,
  onRemind,
}: {
  task: Task;
  account?: WorkspaceData["accounts"][number];
  platforms: WorkspaceData["platforms"];
  onOpen: () => void;
  onStatus: (task: Task, status: TaskStatus) => void;
  onRemind: () => void;
}) {
  const tone = deadlineTone(task.deadline);
  return (
    <article className={`task-row ${task.status !== "pending" ? "task-row-muted" : ""}`}>
      <div className="task-row-title">
        <span className={`platform-icon compact platform-${platformTone(task.platform)}`}>
          {platformInitial(task.platform, platforms)}
        </span>
        <div>
          <strong>{task.title}</strong>
          <small>
            {platformLabel(task.platform, platforms)}
            {account ? ` · ${accountName(account)}` : ""}
          </small>
        </div>
      </div>
      <div className="task-course">
        <strong>{task.course_name || "未分类"}</strong>
        <small>{task.detail || task.source || "学习任务"}</small>
      </div>
      <div className={`task-deadline tone-${tone}`}>
        <span>
          <Clock3 size={14} />
          {formatDateTime(task.deadline)}
        </span>
        <small>{deadlineDescription(task.deadline)}</small>
      </div>
      <div className="task-status">
        {task.status === "pending" ? (
          <Tag tone="accent" dot>待完成</Tag>
        ) : task.status === "ignored" ? (
          <Tag tone="neutral">已忽略</Tag>
        ) : (
          <Tag tone="success">已完成</Tag>
        )}
      </div>
      <div className="task-actions">
        <button className="icon-action open" title="打开平台" aria-label="打开学习任务" onClick={onOpen}>
          <ExternalLink size={16} />
        </button>
        {task.status === "pending" && (
          <>
            <button className="icon-action" title="设置提醒" aria-label="设置任务提醒" onClick={onRemind}>
              <BellPlus size={16} />
            </button>
            <button
              className="icon-action complete"
              title="标记已完成"
              aria-label="标记任务已完成"
              onClick={() => onStatus(task, "completed")}
            >
              <Check size={17} />
            </button>
            <button
              className="icon-action"
              title="忽略任务"
              aria-label="忽略任务"
              onClick={() => onStatus(task, "ignored")}
            >
              <EyeOff size={16} />
            </button>
          </>
        )}
        {task.status !== "pending" && (
          <Button size="small" variant="secondary" onClick={() => onStatus(task, "pending")}>
            恢复
          </Button>
        )}
      </div>
    </article>
  );
}

function PigaiLookup({
  accounts,
  onLookup,
}: {
  accounts: WorkspaceData["accounts"];
  onLookup: (accountId: string, essayId: string) => Promise<void>;
}) {
  const [accountId, setAccountId] = useState(accounts[0]?.id || "");
  const [essayId, setEssayId] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async () => {
    setError("");
    if (!/^\d+$/.test(essayId.trim())) {
      setError("请输入数字作文号");
      return;
    }
    setBusy(true);
    try {
      await onLookup(accountId, essayId.trim());
      setEssayId("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "查询失败");
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="lookup-panel">
      <span className="lookup-icon"><FileSearch size={20} /></span>
      <div className="lookup-copy">
        <strong>批改网作文号</strong>
        <small>直接定位指定作文并添加到本地任务</small>
      </div>
      <Field label="账号">
        <select value={accountId} onChange={(event) => setAccountId(event.target.value)}>
          {accounts.map((account) => (
            <option key={account.id} value={account.id}>{accountName(account)}</option>
          ))}
        </select>
      </Field>
      <Field label="作文号">
        <input
          inputMode="numeric"
          value={essayId}
          onChange={(event) => setEssayId(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void submit();
          }}
          placeholder="例如 3457245"
        />
      </Field>
      <Button variant="primary" onClick={submit} disabled={busy}>
        {busy ? "查询中…" : "查询作文"}
      </Button>
      {error && <span className="form-error lookup-error">{error}</span>}
    </section>
  );
}
