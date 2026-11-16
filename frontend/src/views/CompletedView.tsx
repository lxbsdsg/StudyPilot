import {
  CalendarCheck2,
  CheckCircle2,
  Clock3,
  ExternalLink,
  RotateCcw,
  Search,
  X,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { CompletedTask, WorkspaceData } from "../types";
import {
  accountName,
  deadlineTone,
  formatDateTime,
  formatTimestamp,
  platformTone,
} from "../lib/format";
import { Button, EmptyState, Tag } from "../components/Primitives";
import {
  platformInitial,
  platformLabel,
} from "../components/AppShell";

export function CompletedView({
  data,
  onOpen,
  onRestore,
}: {
  data: WorkspaceData;
  onOpen: (task: CompletedTask) => void;
  onRestore: (task: CompletedTask) => void;
}) {
  const [query, setQuery] = useState("");
  const completed = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return data.completed.filter(
      (task) =>
        !normalized ||
        `${task.title} ${task.course_name} ${task.detail} ${task.source}`
          .toLowerCase()
          .includes(normalized),
    );
  }, [data.completed, query]);
  return (
    <div className="completed-view">
      <section className="completion-summary">
        <span className="completion-icon">
          <CalendarCheck2 size={27} />
        </span>
        <div>
          <span className="eyebrow">LAST 7 DAYS</span>
          <h2>{data.completed.length} 项学习任务已完成</h2>
          <p>
            完成记录会保留七天。
            {data.completed.length
              ? "继续按自己的节奏推进，每一步都算数。"
              : "完成一项学习任务后，它会出现在这里。"}
          </p>
        </div>
        <div className="completion-stat">
          <strong>{data.dashboard.pending}</strong>
          <small>仍待完成</small>
        </div>
      </section>
      <section className="surface completion-board">
        <div className="task-toolbar">
          <div className="search-box completion-search">
            <Search size={17} />
            <input
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索已完成任务"
              aria-label="搜索已完成任务"
            />
            {query && (
              <button aria-label="清除搜索" onClick={() => setQuery("")}>
                <X size={15} />
              </button>
            )}
          </div>
          <span className="completion-hint">
            <CheckCircle2 size={15} />
            完成记录可恢复为待完成
          </span>
        </div>
        {completed.length ? (
          <div className="completed-grid">
            {completed.map((task) => (
              <CompletedCard
                key={task.completion_id || `${task.id}-${task.completed_at}`}
                task={task}
                data={data}
                onOpen={() => onOpen(task)}
                onRestore={() => onRestore(task)}
              />
            ))}
          </div>
        ) : (
          <EmptyState
            icon={<CheckCircle2 size={29} />}
            title={query ? "没有找到匹配的完成记录" : "近七天还没有完成记录"}
            description={
              query
                ? "尝试使用课程名、任务名或平台来源搜索。"
                : "在学习任务页标记完成后，可以在这里回顾记录或恢复任务。"
            }
            action={query ? <Button size="small" onClick={() => setQuery("")}>清除搜索</Button> : undefined}
          />
        )}
      </section>
    </div>
  );
}

function CompletedCard({
  task,
  data,
  onOpen,
  onRestore,
}: {
  task: CompletedTask;
  data: WorkspaceData;
  onOpen: () => void;
  onRestore: () => void;
}) {
  const account = data.accounts.find((item) => item.id === task.account_id);
  return (
    <article className="completed-card">
      <div className="completed-card-top">
        <span className={`platform-icon compact platform-${platformTone(task.platform)}`}>
          {platformInitial(task.platform, data.platforms)}
        </span>
        <div>
          <strong>{task.title}</strong>
          <small>
            {platformLabel(task.platform, data.platforms)} · {accountName(account)}
          </small>
        </div>
        <Tag tone="success">已完成</Tag>
      </div>
      <div className="completed-card-course">
        <span>课程 / 来源</span>
        <strong>{task.course_name || "未分类"}</strong>
        <small>{task.detail || task.source}</small>
      </div>
      <div className="completed-dates">
        <span>
          <CheckCircle2 size={14} />
          完成于 {formatTimestamp(task.completed_at)}
        </span>
        <span className={`tone-${deadlineTone(task.deadline)}`}>
          <Clock3 size={14} />
          截止 {formatDateTime(task.deadline)}
        </span>
      </div>
      <div className="completed-actions">
        <button className="text-link" onClick={onOpen}>
          <ExternalLink size={14} />
          打开平台
        </button>
        <button className="text-link muted-link" onClick={onRestore}>
          <RotateCcw size={14} />
          恢复为待完成
        </button>
      </div>
    </article>
  );
}
