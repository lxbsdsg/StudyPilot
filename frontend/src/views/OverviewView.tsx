import {
  ArrowRight,
  BookOpen,
  CalendarClock,
  CheckCircle2,
  Clock3,
  GraduationCap,
  ListChecks,
  RefreshCw,
  ShieldCheck,
  TimerReset,
} from "lucide-react";
import type { Account, Platform, Task, WorkspaceData } from "../types";
import type { ReactNode } from "react";
import {
  accountName,
  compareTasks,
  deadlineDescription,
  deadlineTone,
  formatDateTime,
} from "../lib/format";
import {
  EmptyState,
  Tag,
} from "../components/Primitives";
import {
  platformInitial,
  platformLabel,
  type ViewId,
} from "../components/AppShell";

export function OverviewView({
  data,
  onView,
  onOpenTask,
  onSyncAll,
  syncing,
}: {
  data: WorkspaceData;
  onView: (view: ViewId) => void;
  onOpenTask: (task: Task) => void;
  onSyncAll: () => void;
  syncing: boolean;
}) {
  const tasks = data.tasks.filter((task) => task.status === "pending");
  const upcoming = [...tasks].sort(compareTasks).slice(0, 5);
  const greeting = getGreeting();
  return (
    <div className="overview-view">
      <section className="welcome-band">
        <div className="welcome-copy">
          <span className="eyebrow">
            <span className="eyebrow-mark" />
            {greeting}
          </span>
          <h2>
            今天，把学习进度
            <br />
            稳稳握在手里。
          </h2>
          <p>
            {tasks.length
              ? `你还有 ${tasks.length} 项学习任务，${data.dashboard.due_soon} 项将在三天内截止。`
              : "所有平台的待办都已清空，可以安排新的学习计划了。"}
          </p>
          <ButtonRow onView={onView} />
        </div>
        <div className="focus-panel" aria-label="近期学习节奏">
          <div className="focus-ring">
            <span>{tasks.length}</span>
            <small>待完成</small>
          </div>
          <div className="focus-details">
            <strong>本周学习节奏</strong>
            <div className="focus-progress">
              <span style={{ width: `${Math.min(100, Math.max(8, data.completed.length * 18))}%` }} />
            </div>
            <small>
              近七天完成 {data.completed.length} 项
              <br />
              {data.dashboard.reminders} 个有效提醒在守护
            </small>
          </div>
          <div className="focus-orbit focus-orbit-one" />
          <div className="focus-orbit focus-orbit-two" />
        </div>
      </section>

      <div className="metric-grid">
        <MetricCard
          icon={<GraduationCap size={19} />}
          label="已连接账号"
          value={data.dashboard.accounts}
          note={`${data.platforms.length} 个学习平台`}
          tone="teal"
        />
        <MetricCard
          icon={<BookOpen size={19} />}
          label="课程总数"
          value={data.dashboard.courses}
          note="已同步至本地"
          tone="blue"
        />
        <MetricCard
          icon={<ListChecks size={19} />}
          label="待完成任务"
          value={data.dashboard.pending}
          note="按截止时间排序"
          tone="amber"
        />
        <MetricCard
          icon={<CalendarClock size={19} />}
          label="三天内截止"
          value={data.dashboard.due_soon}
          note={data.dashboard.due_soon ? "建议优先处理" : "暂时没有紧迫任务"}
          tone="rose"
        />
      </div>

      <div className="overview-grid">
        <section className="surface platform-surface">
          <header className="section-header">
            <div>
              <span className="section-kicker">CONNECTIONS</span>
              <h3>平台状态</h3>
            </div>
            <button className="text-link" onClick={() => onView("accounts")}>
              账号管理
              <ArrowRight size={14} />
            </button>
          </header>
          <div className="platform-list">
            {data.platforms.map((platform) => {
              const accounts = data.accounts.filter((account) => account.platform === platform.id);
              const pending = tasks.filter((task) => task.platform === platform.id).length;
              const check = data.dashboard.platform_checks[platform.id]?.status;
              return (
                <PlatformRow
                  key={platform.id}
                  account={accounts[0]}
                  platform={platform}
                  pending={pending}
                  accountCount={accounts.length}
                  check={check}
                />
              );
            })}
          </div>
          <div className="platform-footnote">
            <ShieldCheck size={15} />
            凭据加密保存在本机，不会同步到云端
          </div>
        </section>
        <section className="surface upcoming-surface">
          <header className="section-header">
            <div>
              <span className="section-kicker">NEXT UP</span>
              <h3>近期任务</h3>
            </div>
            {tasks.length > 0 && (
              <button className="text-link" onClick={() => onView("tasks")}>
                查看全部
                <ArrowRight size={14} />
              </button>
            )}
          </header>
          {upcoming.length ? (
            <div className="task-preview-list">
              {upcoming.map((task, index) => (
                <TaskPreview
                  key={task.id}
                  task={task}
                  account={data.accounts.find((account) => account.id === task.account_id)}
                  platforms={data.platforms}
                  index={index}
                  onOpen={() => onOpenTask(task)}
                />
              ))}
            </div>
          ) : (
            <EmptyState
              icon={<CheckCircle2 size={27} />}
              title="当前没有待完成任务"
              description={
                data.accounts.length
                  ? "同步账号后，新的课程与作业会出现在这里。"
                  : "连接第一个学习平台，开始集中查看课程与待办。"
              }
              action={
                <button className="button button-primary button-small" onClick={onSyncAll} disabled={syncing}>
                  <RefreshCw size={14} className={syncing ? "spin" : ""} />
                  {data.accounts.length ? "同步全部账号" : "添加学习账号"}
                </button>
              }
            />
          )}
        </section>
      </div>

      <section className="focus-strip">
        <span className="focus-strip-icon">
          <TimerReset size={20} />
        </span>
        <div>
          <strong>提前一点，心里更有底</strong>
          <p>为重要作业设置截止前提醒，把时间留给真正需要的部分。</p>
        </div>
        <button className="text-link" onClick={() => onView("reminders")}>
          安排提醒
          <ArrowRight size={14} />
        </button>
      </section>
    </div>
  );
}

function ButtonRow({ onView }: { onView: (view: ViewId) => void }) {
  return (
    <div className="welcome-actions">
      <button className="button button-primary" onClick={() => onView("tasks")}>
        查看学习任务
        <ArrowRight size={15} />
      </button>
      <button className="welcome-link" onClick={() => onView("accounts")}>
        <ShieldCheck size={15} />
        管理平台连接
      </button>
    </div>
  );
}

function MetricCard({
  icon,
  label,
  value,
  note,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: number;
  note: string;
  tone: string;
}) {
  return (
    <article className={`metric-card metric-${tone}`}>
      <div className="metric-icon">{icon}</div>
      <div className="metric-copy">
        <span>{label}</span>
        <strong>{value}</strong>
        <small>{note}</small>
      </div>
    </article>
  );
}

function PlatformRow({
  account,
  platform,
  pending,
  accountCount,
  check,
}: {
  account?: Account;
  platform: Platform;
  pending: number;
  accountCount: number;
  check?: string;
}) {
  const isReady = account?.status === "ready";
  return (
    <div className="platform-row">
      <span className={`platform-icon platform-${platform.id}`}>
        {platformInitial(platform.id, [platform])}
      </span>
      <div className="platform-copy">
        <strong>
          {platform.label}
          {platform.execution_mode === "python-compatibility" && (
            <span className="platform-mode">兼容模式</span>
          )}
        </strong>
        <small>
          {account ? accountName(account) : "尚未添加账号"}
          {accountCount > 1 ? ` · ${accountCount} 个账号` : ""}
        </small>
      </div>
      <div className="platform-meta">
        <span>{pending > 0 ? `${pending} 待办` : "暂无待办"}</span>
        {account && isReady ? (
          <Tag tone="success" dot>已连接</Tag>
        ) : account?.last_error ? (
          <Tag tone="danger" dot>需处理</Tag>
        ) : check ? (
          <Tag tone="warning" dot>待验证</Tag>
        ) : (
          <span className="platform-unlinked">未连接</span>
        )}
      </div>
    </div>
  );
}

function TaskPreview({
  task,
  account,
  platforms,
  index,
  onOpen,
}: {
  task: Task;
  account?: Account;
  platforms: Platform[];
  index: number;
  onOpen: () => void;
}) {
  const tone = deadlineTone(task.deadline);
  return (
    <button className="task-preview" onClick={onOpen}>
      <span className={`task-number tone-${tone}`}>
        {String(index + 1).padStart(2, "0")}
      </span>
      <span className="task-preview-main">
        <strong>{task.title}</strong>
        <small>
          {platformLabel(task.platform, platforms)} · {task.course_name || "未分类"}
          {account ? ` · ${accountName(account)}` : ""}
        </small>
      </span>
      <span className={`task-time tone-${tone}`}>
        <Clock3 size={14} />
        <b>{formatDateTime(task.deadline)}</b>
        <small>{deadlineDescription(task.deadline)}</small>
      </span>
      <ArrowRight className="row-arrow" size={17} />
    </button>
  );
}

function getGreeting() {
  const hour = new Date().getHours();
  if (hour < 6) return "夜深了，记得休息";
  if (hour < 12) return "早上好";
  if (hour < 18) return "下午好";
  return "晚上好";
}
