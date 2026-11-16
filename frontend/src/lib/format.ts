import type { PlatformId, RepeatRule, Task } from "../types";

export function parseLocalDate(value?: string | null): Date | null {
  if (!value) return null;
  const date = new Date(value.includes("T") ? value : value.replace(" ", "T"));
  return Number.isNaN(date.getTime()) ? null : date;
}

export function formatDateTime(value?: string | null) {
  const date = parseLocalDate(value);
  return date
    ? date.toLocaleString("zh-CN", {
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
      })
    : "未设置";
}

export function formatTimestamp(seconds?: number | null) {
  if (!seconds) return "尚未同步";
  return new Date(seconds * 1000).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function deadlineTone(value?: string | null) {
  const date = parseLocalDate(value);
  if (!date) return "neutral";
  const remaining = date.getTime() - Date.now();
  if (remaining < 0) return "danger";
  if (remaining <= 72 * 60 * 60 * 1000) return "warning";
  return "neutral";
}

export function deadlineDescription(value?: string | null) {
  const date = parseLocalDate(value);
  if (!date) return "无截止时间";
  const hours = Math.ceil((date.getTime() - Date.now()) / 3_600_000);
  if (hours < 0) return `已逾期 ${Math.abs(hours)} 小时`;
  if (hours < 24) return `剩余 ${hours} 小时`;
  return `剩余 ${Math.ceil(hours / 24)} 天`;
}

export function accountName(
  account: { label?: string; display_name?: string; username?: string } | undefined,
) {
  return account?.label || account?.display_name || account?.username || "未知账号";
}

export function platformTone(platform: PlatformId) {
  return { pigai: "pigai", unipus: "unipus", welearn: "welearn", chaoxing: "chaoxing" }[
    platform
  ];
}

export const repeatRuleLabels: Record<RepeatRule, string> = {
  once: "仅一次",
  daily: "每天",
  weekdays: "每个工作日",
  weekly: "每周",
};

export const offsetOptions = [
  [10, "截止前 10 分钟"],
  [30, "截止前 30 分钟"],
  [60, "截止前 1 小时"],
  [120, "截止前 2 小时"],
  [1440, "截止前 1 天"],
  [2880, "截止前 2 天"],
  [4320, "截止前 3 天"],
  [10080, "截止前 1 周"],
] as const;

export function toLocalInputValue(date = new Date(Date.now() + 3_600_000)) {
  const offset = date.getTimezoneOffset();
  return new Date(date.getTime() - offset * 60_000)
    .toISOString()
    .slice(0, 16);
}

export function compareTasks(a: Task, b: Task) {
  const leftUpdated = a.updated_at ?? 0;
  const rightUpdated = b.updated_at ?? 0;
  if (!a.deadline && !b.deadline) return rightUpdated - leftUpdated;
  if (!a.deadline) return 1;
  if (!b.deadline) return -1;
  return a.deadline.localeCompare(b.deadline);
}

export type TaskFilter = "pending" | "all" | "ignored";
export type TaskSortMode = "deadline" | "updated" | "title";

export function filterAndSortTasks(
  tasks: Task[],
  filters: {
    status: TaskFilter;
    platform?: string;
    accountId?: string;
    query?: string;
    sort?: TaskSortMode;
  },
) {
  const normalized = filters.query?.trim().toLowerCase() ?? "";
  return tasks
    .filter((task) => (filters.status === "all" ? true : task.status === filters.status))
    .filter((task) => !filters.platform || task.platform === filters.platform)
    .filter((task) => !filters.accountId || task.account_id === filters.accountId)
    .filter(
      (task) =>
        !normalized ||
        `${task.title} ${task.course_name} ${task.detail} ${task.source}`
          .toLowerCase()
          .includes(normalized),
    )
    .sort((left, right) => {
      if (filters.sort === "title") return left.title.localeCompare(right.title, "zh-CN");
      if (filters.sort === "updated") {
        return (right.updated_at ?? 0) - (left.updated_at ?? 0);
      }
      return compareTasks(left, right);
    });
}
